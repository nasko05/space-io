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
KEY_PATH="${HEARTH_KEY_PATH:-}"
ALLOWED_CIDR="${HEARTH_ALLOWED_CIDR:-}"
INSTANCE_TYPE="${HEARTH_INSTANCE_TYPE:-t4g.nano}"
DATA_GIB="${HEARTH_DATA_GIB:-8}"
REPO_URL="${HEARTH_REPO_URL:-https://github.com/nasko05/space-io.git}"
REPO_REF="${HEARTH_REPO_REF:-main}"
HEARTH_PORT="${HEARTH_PORT:-7777}"
TEMPLATE="$(dirname "$0")/cloudformation.yaml"

# If the operator created the key pair via `aws ec2 create-key-pair` they
# typically saved it to ~/.ssh/<keypair>.pem (that's what our README says to
# do). Auto-detect that path so plain `ssh` doesn't fall back to id_rsa /
# id_ecdsa and trip "Permission denied (publickey)".
if [ -z "$KEY_PATH" ] && [ -n "$KEY_PAIR" ] && [ -f "$HOME/.ssh/${KEY_PAIR}.pem" ]; then
  KEY_PATH="$HOME/.ssh/${KEY_PAIR}.pem"
fi

# Silently load KEY_PATH into ssh-agent (if one is running) so other tools
# -- plain `ssh`, `scp`, `rsync`, your editor's remote plugin -- also pick
# it up. Belt and braces with the explicit -i flag we still thread into
# every ssh invocation below: the agent is the convenience layer, -i is the
# correctness layer.
ensure_key_in_agent() {
  [ -z "$KEY_PATH" ] && return 0
  [ -f "$KEY_PATH" ] || return 0
  [ "${HEARTH_NO_SSH_ADD:-}" = "1" ] && return 0
  command -v ssh-add >/dev/null 2>&1 || return 0
  # ssh-add -l exits 2 when no agent is reachable; 0 = some keys; 1 = none.
  ssh-add -l >/dev/null 2>&1
  local rc=$?
  if [ $rc -eq 2 ]; then
    return 0  # no agent running, skip silently; -i will still work
  fi
  # Compute the key's SHA256 fingerprint and check the agent for it.
  local fp
  fp=$(ssh-keygen -lf "$KEY_PATH" 2>/dev/null | awk '{print $2}')
  if [ -n "$fp" ] && ssh-add -l 2>/dev/null | grep -qF "$fp"; then
    return 0  # already loaded
  fi
  # On macOS persist into Keychain so a reboot doesn't undo it.
  if [ "$(uname -s)" = "Darwin" ] && ssh-add --apple-use-keychain "$KEY_PATH" 2>/dev/null; then
    echo "Loaded $KEY_PATH into ssh-agent (persisted in macOS Keychain)."
  elif ssh-add "$KEY_PATH" 2>/dev/null; then
    echo "Loaded $KEY_PATH into ssh-agent."
  fi
}
ensure_key_in_agent

usage() {
  cat <<'USAGE'
Usage: deploy/deploy.sh [--profile NAME] [--region REGION] [--env-file PATH] <command>

Commands:
  up        create or update the stack
  down      delete the stack (the data volume is RETAINED — see notes)
  status    show stack + instance state
  whoami    print the AWS identity + profile + region the script will use
  ssh       open an SSH session to the running instance
  logs      tail the bootstrap log
  open      print the public URL (http://<eip>:<port>) for the instance

Config file:
  deploy/.env is auto-sourced if it exists (use deploy/.env.example as a template).
  Override with --env-file PATH or HEARTH_ENV_FILE=PATH.
  Disable with --no-env-file.

Required env (for `up`):
  HEARTH_KEYPAIR        existing EC2 key pair name (for SSH)
  HEARTH_ALLOWED_CIDR   CIDR allowed to reach SSH/Hearth (no default — set
                        to your /32 unless you really want a public host)

AWS auth (any one is enough; --profile and --region win over env):
  --profile NAME        named profile from ~/.aws/config / ~/.aws/credentials
  --region NAME         target region
  HEARTH_AWS_PROFILE    same as --profile, env form
  AWS_PROFILE           standard AWS CLI variable (also honoured)
  AWS_REGION            target region (default: us-east-1)
  IAM role / SSO        anything the AWS SDK credential chain resolves

Optional env:
  HEARTH_STACK          CloudFormation stack name (default: hearth)
  HEARTH_KEY_PATH       Path to the SSH private key for the keypair.
                        Auto-detected as ~/.ssh/${HEARTH_KEYPAIR}.pem
                        if present; set explicitly otherwise.
  HEARTH_NO_SSH_ADD     Set to 1 to skip the implicit `ssh-add` of the
                        resolved key into your ssh-agent. The script
                        still threads -i into its own ssh calls.
  HEARTH_INSTANCE_TYPE  EC2 type (default: t4g.nano -- ARM, cheapest)
  HEARTH_DATA_GIB       data volume size in GiB (default: 8)
  HEARTH_REPO_URL       git URL the instance clones (default: this repo)
  HEARTH_REPO_REF       branch or tag (default: main)
  HEARTH_PORT           port Hearth listens on (default: 7777)
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
  5. Print the URL to open:
       deploy/deploy.sh open
MSG
}

cmd_down() {
  verify_auth
  echo "About to delete stack '$STACK_NAME' in $REGION."
  echo "The /data EBS volume is retained on stack delete (see template),"
  echo "so the encrypted vault survives -- but the instance, EIP, and SG"
  echo "are torn down. You'll need to re-attach the volume to bring it back."
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

# Build the ssh argv prefix. Always wraps -o StrictHostKeyChecking=accept-new
# so a fresh EIP doesn't prompt; threads -i + IdentitiesOnly=yes when a key
# path is resolved (so ssh-agent's many keys don't burn through MaxAuthTries
# before AWS lets us in).
SSH_BASE_OPTS=(-o StrictHostKeyChecking=accept-new)
ssh_to() {
  local ip=$1; shift
  if [ -n "$KEY_PATH" ]; then
    if [ ! -f "$KEY_PATH" ]; then
      echo "error: HEARTH_KEY_PATH does not exist: $KEY_PATH" >&2
      exit 1
    fi
    exec ssh "${SSH_BASE_OPTS[@]}" -i "$KEY_PATH" -o IdentitiesOnly=yes \
      "ec2-user@$ip" "$@"
  else
    exec ssh "${SSH_BASE_OPTS[@]}" "ec2-user@$ip" "$@"
  fi
}

cmd_ssh() {
  local ip
  ip=$(instance_ip)
  [ -n "$ip" ] || { echo "stack has no PublicAddress output yet" >&2; exit 1; }
  ssh_to "$ip"
}

cmd_logs() {
  local ip
  ip=$(instance_ip)
  [ -n "$ip" ] || { echo "stack has no PublicAddress output yet" >&2; exit 1; }
  ssh_to "$ip" 'sudo tail -n 200 -f /var/log/hearth-bootstrap.log'
}

# Hearth listens directly on its public IP; `open` just hands you the URL.
# No SSH tunnel: traffic is plain HTTP, which is fine for a single-tenant
# host you reach over your own VPN, but means the passphrase is in the
# clear if you typed it onto the open internet. Front the instance with
# Caddy/nginx/Cloudflare Tunnel if you want TLS.
cmd_open() {
  local ip
  ip=$(instance_ip)
  [ -n "$ip" ] || { echo "stack has no PublicAddress output yet" >&2; exit 1; }
  echo "http://${ip}:${HEARTH_PORT}"
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
  open) cmd_open ;;
  *) echo "unknown command: ${ARGS[0]}" >&2; usage; exit 1 ;;
esac
