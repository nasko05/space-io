#!/usr/bin/env bash
# Minimal one-command deploy for SpaceIO Hearth on AWS.
# Creates a CloudFormation stack with one EC2 instance, one security group,
# one EBS volume, and one Elastic IP. The instance clones the repo and
# builds it on first boot; you SSH in once to run `init-space.sh`.

set -euo pipefail

# Auto-load deploy/.env (or $HEARTH_ENV_FILE, or whatever --env-file
# points at) so you don't have to re-export HEARTH_KEYPAIR every
# session. We do this BEFORE reading any HEARTH_* variables below.
load_env_file() {
  local target="$1"
  [ -z "$target" ] && return 0
  if [ ! -f "$target" ]; then
    echo "error: env file not found: $target" >&2
    exit 1
  fi
  # shellcheck disable=SC1090
  set -a
  source "$target"
  set +a
}

# Pull --env-file off the front of the args first so it can supply
# HEARTH_* defaults that the rest of the script reads.
ENV_FILE="${HEARTH_ENV_FILE:-}"
DEFAULT_ENV_FILE="$(dirname "$0")/.env"
ENV_ARGS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --env-file)
      [ $# -ge 2 ] || { echo "error: --env-file needs a path" >&2; exit 1; }
      ENV_FILE="$2"; shift 2 ;;
    --env-file=*)
      ENV_FILE="${1#--env-file=}"; shift ;;
    --no-env-file)
      ENV_FILE="-"; shift ;;
    *)
      ENV_ARGS+=("$1"); shift ;;
  esac
done

if [ "$ENV_FILE" = "-" ]; then
  : # explicitly disabled
elif [ -n "$ENV_FILE" ]; then
  load_env_file "$ENV_FILE"
elif [ -f "$DEFAULT_ENV_FILE" ]; then
  load_env_file "$DEFAULT_ENV_FILE"
fi

set -- "${ENV_ARGS[@]:-}"

STACK_NAME="${HEARTH_STACK:-hearth}"
REGION="${AWS_REGION:-${AWS_DEFAULT_REGION:-us-east-1}}"
PROFILE="${HEARTH_AWS_PROFILE:-${AWS_PROFILE:-}}"
KEY_PAIR="${HEARTH_KEYPAIR:-}"
ALLOWED_CIDR="${HEARTH_ALLOWED_CIDR:-}"
INSTANCE_TYPE="${HEARTH_INSTANCE_TYPE:-t4g.nano}"
DATA_GIB="${HEARTH_DATA_GIB:-8}"
REPO_URL="${HEARTH_REPO_URL:-https://github.com/nasko05/space-io.git}"
REPO_REF="${HEARTH_REPO_REF:-main}"
TEMPLATE="$(dirname "$0")/cloudformation.yaml"

usage() {
  cat <<'USAGE'
Usage: deploy/deploy.sh [--profile NAME] [--region REGION] [--env-file PATH] <command>

Commands:
  up        create or update the stack
  down      delete the stack (and the data volume!)
  status    show stack + instance state
  whoami    print the AWS identity + profile + region the script will use
  ssh       open an SSH session to the running instance
  logs      tail the bootstrap log

Config file:
  deploy/.env is auto-sourced if it exists (use deploy/.env.example as a template).
  Override with --env-file PATH or HEARTH_ENV_FILE=PATH.
  Disable with --no-env-file.

Required env (for `up`):
  HEARTH_KEYPAIR        existing EC2 key pair name (for SSH)

AWS auth (any one is enough; --profile and --region win over env):
  --profile NAME        named profile from ~/.aws/config / ~/.aws/credentials
  --region NAME         target region
  HEARTH_AWS_PROFILE    same as --profile, env form
  AWS_PROFILE           standard AWS CLI variable (also honoured)
  AWS_REGION            target region (default: us-east-1)
  IAM role / SSO        anything the AWS SDK credential chain resolves

Optional env:
  HEARTH_STACK          CloudFormation stack name (default: hearth)
  HEARTH_ALLOWED_CIDR   CIDR allowed to reach SSH/Hearth
                        (default: your current public IP /32)
  HEARTH_INSTANCE_TYPE  EC2 type (default: t4g.nano — ARM, cheapest)
  HEARTH_DATA_GIB       data volume size in GiB (default: 8)
  HEARTH_REPO_URL       git URL the instance clones (default: this repo)
  HEARTH_REPO_REF       branch or tag (default: main)
USAGE
}

require() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: $1 not found in PATH" >&2; exit 1; }
}

# Front-end arg parse: pull --profile / --region (in either order) off the
# front of the args so subcommands stay positional. Everything after the
# first non-flag is the subcommand.
parse_flags() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --profile)
        [ $# -ge 2 ] || { echo "error: --profile needs a name" >&2; exit 1; }
        PROFILE="$2"; shift 2 ;;
      --profile=*)
        PROFILE="${1#--profile=}"; shift ;;
      --region)
        [ $# -ge 2 ] || { echo "error: --region needs a name" >&2; exit 1; }
        REGION="$2"; shift 2 ;;
      --region=*)
        REGION="${1#--region=}"; shift ;;
      -h|--help)
        # Promote help-flag to the `help` command so the normal dispatch
        # path handles it (without requiring aws / curl).
        ARGS=("help"); return ;;
      --)
        shift; break ;;
      -*)
        echo "unknown option: $1" >&2; usage; exit 1 ;;
      *)
        break ;;
    esac
  done
  ARGS=("$@")
}

# Every `aws` invocation goes through this so --profile / --region / region
# env are applied uniformly.
aws_() {
  if [ -n "$PROFILE" ]; then
    aws --profile "$PROFILE" --region "$REGION" "$@"
  else
    aws --region "$REGION" "$@"
  fi
}

# Hits STS once to fail fast with a friendly message if creds aren't set up,
# and to print which account/identity will own the resources.
verify_auth() {
  local out
  if ! out=$(aws_ sts get-caller-identity --output json 2>&1); then
    cat >&2 <<EOF
error: AWS authentication failed.
  $out

  profile: ${PROFILE:-(none — using default credential chain)}
  region:  $REGION

Try one of:
  aws configure                          # set up a default profile
  aws sso login --profile $PROFILE       # if using SSO
  export AWS_ACCESS_KEY_ID=... AWS_SECRET_ACCESS_KEY=...
EOF
    exit 1
  fi
  local account user
  account=$(printf '%s' "$out" | sed -n 's/.*"Account": "\([^"]*\)".*/\1/p')
  user=$(printf '%s' "$out" | sed -n 's/.*"Arn": "\([^"]*\)".*/\1/p')
  echo "Authenticated as ${user:-?} (account ${account:-?})"
  echo "  profile: ${PROFILE:-(default credential chain)}"
  echo "  region:  $REGION"
}

ensure_cidr() {
  if [ -n "$ALLOWED_CIDR" ]; then
    return
  fi
  local ip
  ip=$(curl -fsSL https://checkip.amazonaws.com 2>/dev/null | tr -d '[:space:]' || true)
  if [ -z "$ip" ]; then
    echo "error: could not detect your public IP; set HEARTH_ALLOWED_CIDR explicitly" >&2
    exit 1
  fi
  ALLOWED_CIDR="${ip}/32"
  echo "Locking SG to your current IP: ${ALLOWED_CIDR}"
}

cmd_whoami() {
  verify_auth
}

cmd_up() {
  [ -n "$KEY_PAIR" ] || { echo "error: set HEARTH_KEYPAIR to an existing EC2 key pair name" >&2; exit 1; }
  verify_auth
  ensure_cidr
  aws_ cloudformation deploy \
    --stack-name "$STACK_NAME" \
    --template-file "$TEMPLATE" \
    --capabilities CAPABILITY_IAM \
    --parameter-overrides \
      "KeyPairName=$KEY_PAIR" \
      "AllowedCidr=$ALLOWED_CIDR" \
      "InstanceType=$INSTANCE_TYPE" \
      "DataVolumeGiB=$DATA_GIB" \
      "RepositoryUrl=$REPO_URL" \
      "RepositoryRef=$REPO_REF"
  cmd_status
  cat <<MSG

Stack is up. Next:
  1. Wait ~3-5 minutes for the bootstrap script to build the binary.
     Tail with: deploy/deploy.sh logs
  2. SSH in:    deploy/deploy.sh ssh
  3. Initialise the space (one-time, interactive — asks for a passphrase):
       /opt/space-io/init-space.sh
  4. Start the service:
       sudo systemctl start hearth
  5. Open the URL from 'deploy/deploy.sh status' in a browser.
MSG
}

cmd_down() {
  verify_auth
  echo "About to delete stack '$STACK_NAME' in $REGION."
  echo "This DESTROYS the EBS volume holding your encrypted data."
  read -r -p "Type 'yes' to confirm: " ans
  [ "$ans" = "yes" ] || { echo "aborted"; exit 1; }
  aws_ cloudformation delete-stack --stack-name "$STACK_NAME"
  aws_ cloudformation wait stack-delete-complete --stack-name "$STACK_NAME"
  echo "Stack deleted."
}

cmd_status() {
  local outputs
  outputs=$(aws_ cloudformation describe-stacks --stack-name "$STACK_NAME" \
    --query 'Stacks[0].Outputs' --output table 2>/dev/null || true)
  if [ -z "$outputs" ]; then
    echo "stack '$STACK_NAME' not found in $REGION"
    return 1
  fi
  echo "$outputs"
}

instance_ip() {
  aws_ cloudformation describe-stacks --stack-name "$STACK_NAME" \
    --query 'Stacks[0].Outputs[?OutputKey==`PublicAddress`].OutputValue' --output text 2>/dev/null
}

cmd_ssh() {
  local ip
  ip=$(instance_ip)
  [ -n "$ip" ] || { echo "stack has no PublicAddress output yet" >&2; exit 1; }
  exec ssh -o StrictHostKeyChecking=accept-new "ec2-user@$ip"
}

cmd_logs() {
  local ip
  ip=$(instance_ip)
  [ -n "$ip" ] || { echo "stack has no PublicAddress output yet" >&2; exit 1; }
  exec ssh -o StrictHostKeyChecking=accept-new "ec2-user@$ip" \
    'sudo tail -n 200 -f /var/log/hearth-bootstrap.log'
}

parse_flags "$@"

# Help is the only command that doesn't need the aws / curl toolchain.
case "${ARGS[0]:-}" in
  ""|-h|--help|help) usage; exit 0 ;;
esac

require aws
require curl

case "${ARGS[0]:-}" in
  up) cmd_up ;;
  down) cmd_down ;;
  status) cmd_status ;;
  whoami) cmd_whoami ;;
  ssh) cmd_ssh ;;
  logs) cmd_logs ;;
  *) echo "unknown command: ${ARGS[0]}" >&2; usage; exit 1 ;;
esac
