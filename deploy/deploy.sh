#!/usr/bin/env bash
# Minimal one-command deploy for SpaceIO Hearth on AWS.
# Creates a CloudFormation stack with one EC2 instance, one security group,
# one EBS volume, and one Elastic IP. The instance clones the repo and
# builds it on first boot; you SSH in once to run `init-space.sh`.

set -euo pipefail

STACK_NAME="${HEARTH_STACK:-hearth}"
REGION="${AWS_REGION:-${AWS_DEFAULT_REGION:-us-east-1}}"
KEY_PAIR="${HEARTH_KEYPAIR:-}"
ALLOWED_CIDR="${HEARTH_ALLOWED_CIDR:-}"
INSTANCE_TYPE="${HEARTH_INSTANCE_TYPE:-t4g.nano}"
DATA_GIB="${HEARTH_DATA_GIB:-8}"
REPO_URL="${HEARTH_REPO_URL:-https://github.com/nasko05/space-io.git}"
REPO_REF="${HEARTH_REPO_REF:-main}"
TEMPLATE="$(dirname "$0")/cloudformation.yaml"

usage() {
  cat <<'USAGE'
Usage: deploy/deploy.sh [up|down|status|ssh|logs]

Required env:
  HEARTH_KEYPAIR        existing EC2 key pair name (for SSH)

Optional env:
  HEARTH_STACK          CloudFormation stack name (default: hearth)
  AWS_REGION            target region (default: us-east-1)
  HEARTH_ALLOWED_CIDR   CIDR allowed to reach SSH/Hearth (default: your current public IP /32)
  HEARTH_INSTANCE_TYPE  EC2 type (default: t4g.nano — ARM, cheapest)
  HEARTH_DATA_GIB       data volume size in GiB (default: 8)
  HEARTH_REPO_URL       git URL the instance clones (default: this repo)
  HEARTH_REPO_REF       branch or tag (default: main)

Commands:
  up        create or update the stack
  down      delete the stack (and the data volume!)
  status    show stack + instance state
  ssh       open an SSH session to the running instance
  logs      tail the bootstrap log
USAGE
}

require() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: $1 not found in PATH" >&2; exit 1; }
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

cmd_up() {
  [ -n "$KEY_PAIR" ] || { echo "error: set HEARTH_KEYPAIR to an existing EC2 key pair name" >&2; exit 1; }
  ensure_cidr
  aws cloudformation deploy \
    --region "$REGION" \
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
  echo "About to delete stack '$STACK_NAME' in $REGION."
  echo "This DESTROYS the EBS volume holding your encrypted data."
  read -r -p "Type 'yes' to confirm: " ans
  [ "$ans" = "yes" ] || { echo "aborted"; exit 1; }
  aws cloudformation delete-stack --region "$REGION" --stack-name "$STACK_NAME"
  aws cloudformation wait stack-delete-complete --region "$REGION" --stack-name "$STACK_NAME"
  echo "Stack deleted."
}

cmd_status() {
  local outputs
  outputs=$(aws cloudformation describe-stacks --region "$REGION" --stack-name "$STACK_NAME" \
    --query 'Stacks[0].Outputs' --output table 2>/dev/null || true)
  if [ -z "$outputs" ]; then
    echo "stack '$STACK_NAME' not found in $REGION"
    return 1
  fi
  echo "$outputs"
}

instance_ip() {
  aws cloudformation describe-stacks --region "$REGION" --stack-name "$STACK_NAME" \
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

require aws
require curl

case "${1:-}" in
  up) cmd_up ;;
  down) cmd_down ;;
  status) cmd_status ;;
  ssh) cmd_ssh ;;
  logs) cmd_logs ;;
  ""|-h|--help|help) usage ;;
  *) echo "unknown command: $1" >&2; usage; exit 1 ;;
esac
