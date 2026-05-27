# Contributing to SpaceIO Hearth

Thanks for opening the project. Hearth is meant to be self-hosted — the
expectation is that the person running it is also the person modifying
it. This file is short because most contributing patterns flow out of
that.

## Layout

```
Cargo.toml        # single-crate Rust backend
src/              # axum router, crypto, git, file operations
web/              # Vite + React + TypeScript frontend
  src/            # components, lib helpers
  dist/           # built bundle (gitignored, embedded into the binary)
deploy/           # CloudFormation + shell for the minimal AWS deployment
.github/workflows # CI
```

The Rust binary embeds `web/dist/` at compile time via `rust-embed`, so
the frontend has to build before the backend.

## Building

```sh
# One-time: install npm packages
cd web && npm install && cd ..

# Build the frontend
cd web && npm run build && cd ..

# Build the backend
cargo build --release

# Or, for development, run them separately:
cd web && npm run dev      # Vite dev server on :5173, proxies /api → :7777
cargo run -- serve --space-dir ./data --listen 127.0.0.1:7777
```

## Running locally

```sh
./target/release/hearth init --space-dir ./data        # one-time, prompts for passphrase
./target/release/hearth serve --space-dir ./data       # open http://127.0.0.1:7777
```

`./data` is gitignored. You can blow it away and re-init without
touching the repo.

## CI gates

CI runs on every PR against `main`. Before opening a PR, run locally:

```sh
cd web && npm ci && npm run build && cd ..
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Any of those four failing will fail the PR check. `cargo fmt` and
`clippy` are pinned with `-D warnings`, so a new warning is a build
break.

## Branch + PR convention

- Branch off `main`. Branch name doesn't matter, but `topic/short-name`
  reads well.
- One logical change per PR. The diff is the conversation; keep it
  small and the description honest.
- The PR body should answer two questions: *what does this change
  do?* and *how did you verify it?*

## Security-sensitive code

A few areas where a "harmless" refactor can quietly become a security
bug. These have tests pinned for exactly that reason — if you touch
them, please make sure the tests still pass and add new ones if you've
changed the contract.

| File | Invariant |
|---|---|
| `src/space/paths.rs` | Every external path argument resolves under the space root. `..`, absolute paths, and symlinks escaping the root must return `Forbidden`. |
| `src/crypto/age_io.rs` | Encrypt → decrypt with the same passphrase is the identity. Wrong passphrase must fail loudly, not return garbage. |
| `src/routes/auth.rs` | Passphrase verification uses constant-time comparison (`kdf::verify`). The session cookie is `HttpOnly` + `SameSite=Strict`. |
| `web/src/lib/passkey.ts` | The PRF output never leaves the browser; only the wrapped passphrase + salt + credential ID hit the wire. |

If you're unsure whether your change is in this category, it probably
is — open the PR as a draft and ask.

## Threat model reminders (the short version)

- The server stores **ciphertext only** at rest. Decryption happens in
  memory while a session is held.
- The passphrase is **never** committed to the repo, sent through
  CloudFormation parameters, stored in instance metadata, or written
  to any persistent log.
- WebAuthn is an **alternate path** to recover the passphrase
  end-to-end through the browser, not a key-management replacement.
  The server still encrypts files with the passphrase.

## Where to find context

- `SPEC.md` (in the original UI-team bundle) — product spec.
- PR descriptions — every shipped PR explains the design decision and
  the test plan. Reading recent merged PRs is the fastest way to learn
  how a piece of the system was built.

## License

Same as the repository's `LICENSE` file (or `UNLICENSED` if missing —
add one before opening a PR that adds source files).
