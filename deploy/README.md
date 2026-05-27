# SpaceIO Hearth · AWS deployment

A one-stack deployment. Three resources, plus an implicit EBS root volume:

| Resource | Why |
|---|---|
| EC2 instance (`t4g.nano` by default) | Runs the Hearth binary |
| Security Group | SSH + port 7777, locked to your CIDR |
| Elastic IP | Stable address; survives stop/start |
| (root EBS, gp3, encrypted) | Disk + your `.age` blobs |

No load balancer, no Route53, no ACM certificate, no S3, no IAM role.
Monthly cost on `us-east-1` runs about **\$4–\$5** depending on volume size.

## Prereqs

- `aws` CLI installed and able to authenticate (see below)
- An EC2 key pair in the target region (see "Get a key pair" below)

## Get a key pair

You need an SSH key pair in EC2 that the instance will trust. The
script reads its name from `HEARTH_KEYPAIR`. Three ways, pick one:

### A. Generate a new pair in AWS and download the private key

Quickest if you don't already have an SSH key you want to reuse:

```sh
aws ec2 create-key-pair \
  --profile dev-creds --region us-east-1 \
  --key-name hearth \
  --query 'KeyMaterial' --output text \
  > ~/.ssh/hearth.pem
chmod 600 ~/.ssh/hearth.pem
```

Then put `HEARTH_KEYPAIR=hearth` in `deploy/.env`. Later you'll SSH
in with `ssh -i ~/.ssh/hearth.pem ec2-user@<ip>` — or the script's
`deploy/deploy.sh ssh` if `~/.ssh/hearth.pem` is your default key.

### B. Import your existing SSH public key

Nicer if you already have `~/.ssh/id_ed25519` (or `id_rsa`) — no new
private key to manage, plain `ssh ec2-user@<ip>` Just Works:

```sh
aws ec2 import-key-pair \
  --profile dev-creds --region us-east-1 \
  --key-name hearth \
  --public-key-material fileb://~/.ssh/id_ed25519.pub
```

Then `HEARTH_KEYPAIR=hearth` in `deploy/.env`.

### C. Use one you already created

```sh
aws ec2 describe-key-pairs \
  --profile dev-creds --region us-east-1 \
  --query 'KeyPairs[].KeyName' --output table
```

Pick a name from the list and put it in `HEARTH_KEYPAIR`.

> ⚠️ **Region-scoped.** A key pair lives in one region; if you change
> `AWS_REGION` later you'll need to create or import it there too.

## Authentication

The script doesn't bake any credentials in — every `aws` call goes
through the standard AWS credential chain. Pick whichever fits your
setup:

| Setup | What to do |
|---|---|
| **Named profile** (most common) | `deploy/deploy.sh --profile work up` or `export AWS_PROFILE=work` |
| **AWS SSO / IAM Identity Center** | `aws sso login --profile work`, then as above |
| **Default profile** (single account) | `aws configure` once, then run with no flag |
| **Static access keys** | `export AWS_ACCESS_KEY_ID=… AWS_SECRET_ACCESS_KEY=…` |
| **GitHub Actions** | `aws-actions/configure-aws-credentials@v4` with OIDC |
| **CI runner with an instance role** | Nothing — the SDK chain picks it up |

The script accepts both `--profile NAME` and `--region NAME` flags
*before* the subcommand, and equivalent env vars
(`HEARTH_AWS_PROFILE`, standard `AWS_PROFILE`, `AWS_REGION`).
The flag wins over the env var.

Sanity-check the resolved identity before you spend any AWS dollars:

```sh
deploy/deploy.sh --profile work whoami
# Authenticated as arn:aws:iam::123456789012:user/ada (account 123456789012)
#   profile: work
#   region:  us-east-1
```

If creds are missing or expired, `whoami` (and every other command)
fails early with a friendly message and the most likely fix.

## Config file (recommended)

Copy the example and edit:

```sh
cp deploy/.env.example deploy/.env
$EDITOR deploy/.env
```

`deploy/.env` is **gitignored** and auto-sourced by `deploy/deploy.sh`
on every run. Put your profile name, region, key-pair name, and any
other `HEARTH_*` settings in there once and forget them. Override with
`--env-file PATH` (or `HEARTH_ENV_FILE`), or skip with `--no-env-file`
for a clean run from pure CLI flags + env.

## Bring it up

```sh
# After deploy/.env is filled in:
deploy/deploy.sh up

# Or fully explicit, no .env file:
deploy/deploy.sh --no-env-file --profile work --region eu-west-1 up
# (with HEARTH_KEYPAIR set in your shell env)
```

The script auto-detects your public IP and locks the security group to it.
Override with `HEARTH_ALLOWED_CIDR=203.0.113.0/24` if you need a wider range.

First boot installs Rust + Node and builds the binary — about 3–5 minutes.
Watch progress:

```sh
deploy/deploy.sh logs
```

## Initialise the space (one-time, interactive)

```sh
deploy/deploy.sh ssh
# on the instance:
/opt/space-io/init-space.sh        # prompts for a passphrase
sudo systemctl start hearth
```

The passphrase is **only** typed during init and at unlock — it never
lives in CloudFormation parameters or instance metadata, by design.

## Open the app

```sh
deploy/deploy.sh open
```

Opens an SSH tunnel (local `7777` → instance `7777`) and pops a
browser tab at `http://127.0.0.1:7777`. The tunnel stays alive until
you Ctrl-C the terminal — close the tab any time, the connection
re-opens with the next click.

> 🔐 Why a tunnel? Hearth serves plain HTTP on port 7777. Without the
> tunnel, your **passphrase travels in cleartext** over the public
> internet on every unlock. The tunnel routes it through SSH instead,
> so it's encrypted end-to-end without needing TLS certs.

If you've already set up TLS (Caddy on the box, CloudFront, etc.) and
want the public URL, opt out:

```sh
deploy/deploy.sh open --no-tunnel
# (script prompts you to confirm because the default is insecure)
```

## Day-2 ops

```sh
deploy/deploy.sh whoami        # identity / profile / region
deploy/deploy.sh status        # outputs (IP, SSH cmd, URL)
deploy/deploy.sh ssh           # SSH in
deploy/deploy.sh logs          # tail bootstrap log

# on the instance
sudo systemctl status hearth   # service health
sudo journalctl -u hearth -f   # live server logs
```

All of the above pull profile/region from `deploy/.env`; add
`--profile NAME` or `--region NAME` to override for a single call.

## Updating

To redeploy with a different ref:

```sh
HEARTH_REPO_REF=v0.2.0 deploy/deploy.sh up   # CloudFormation no-op if same
deploy/deploy.sh ssh
cd /opt/space-io
git fetch origin && git checkout v0.2.0
(cd web && npm install && npm run build)
PATH=/opt/cargo/bin:$PATH cargo build --release
sudo systemctl restart hearth
```

(CloudFormation only reruns the user-data on a fresh instance, so for content
updates you SSH in and pull yourself.)

## Tear it down

```sh
deploy/deploy.sh down
```

**This destroys the EBS volume**. Snapshot or rsync your `data/` away first
if you want it back.

## TLS

The default deployment serves plain HTTP on port 7777. Three paths to
end-to-end encryption, from cheapest to nicest:

1. **SSH tunnel** (built in — `deploy/deploy.sh open`). Already
   documented above. No DNS, no certs, no extra AWS resources.
2. **Caddy on the instance** — point a domain at the Elastic IP, then
   `sudo dnf install caddy && sudo caddy reverse-proxy --from your.domain --to localhost:7777`.
   Auto Let's Encrypt. After this, `deploy/deploy.sh open --no-tunnel`
   gets you to the real domain.
3. **CloudFront + ACM** — adds resources (and cost). Not minimal; left
   as an exercise.

## Security notes

- The on-disk `.age` blobs are encrypted before the EBS layer sees them; EBS
  encryption is a defence-in-depth measure, not the primary boundary.
- The security group should be locked to your IP. `0.0.0.0/0` defeats the
  premise.
- The instance role grants nothing; the binary doesn't talk to any AWS
  service.
