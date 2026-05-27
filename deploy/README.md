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

- `aws` CLI installed and authenticated (`aws sts get-caller-identity` should work)
- An existing EC2 key pair in the target region

## Bring it up

```sh
export HEARTH_KEYPAIR=my-key            # required
export AWS_REGION=us-east-1             # optional, default us-east-1
deploy/deploy.sh up
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

Then open the URL from `deploy/deploy.sh status`. The passphrase is **only**
typed during init and at unlock — it never lives in CloudFormation parameters
or instance metadata, by design.

## Day-2 ops

```sh
deploy/deploy.sh status        # show outputs (IP, SSH command, URL)
deploy/deploy.sh ssh           # SSH in
deploy/deploy.sh logs          # tail bootstrap log

# on the instance
sudo systemctl status hearth   # service health
sudo journalctl -u hearth -f   # live server logs
```

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

The default deployment serves plain HTTP on port 7777. Three paths to TLS,
from cheapest to nicest:

1. **SSH tunnel** — `ssh -L 7777:127.0.0.1:7777 ec2-user@<ip>` and open
   `http://127.0.0.1:7777` locally. No DNS, no certs, no extra resources.
2. **Caddy on the instance** — point a domain at the Elastic IP, then
   `sudo dnf install caddy && sudo caddy reverse-proxy --from your.domain --to localhost:7777`.
   Auto Let's Encrypt.
3. **CloudFront + ACM** — adds resources (and cost). Not minimal; left as an
   exercise.

## Security notes

- The on-disk `.age` blobs are encrypted before the EBS layer sees them; EBS
  encryption is a defence-in-depth measure, not the primary boundary.
- The security group should be locked to your IP. `0.0.0.0/0` defeats the
  premise.
- The instance role grants nothing; the binary doesn't talk to any AWS
  service.
