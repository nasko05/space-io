mod config;
mod crypto;
mod error;
mod routes;
mod space;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;

use age::secrecy::SecretString;
use anyhow::Context;
use clap::{Parser, Subcommand};

use crate::space::session::SessionStore;
use crate::space::users::{normalise_email, UsersRegistry};
use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(
    name = "hearth",
    version,
    about = "SpaceIO · Hearth — self-hosted personal repository"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Register a new user from the CLI. Mints a UUID, creates the per-user
    /// space directory, and records the email→UUID mapping in `.users.toml`.
    /// The browser registration page is the primary path; this is for
    /// scripted setups.
    Init {
        /// Root directory holding `.users.toml` and the per-user subdirs.
        #[arg(long, default_value = "./data")]
        space_dir: PathBuf,
        /// Email — used as the unique identifier on the login screen.
        #[arg(long)]
        email: String,
        /// Passphrase. If omitted, prompted interactively (with confirm).
        #[arg(long)]
        passphrase: Option<String>,
        /// Optional display name. Defaults to the email.
        #[arg(long)]
        owner: Option<String>,
    },
    /// Serve over HTTP. Tolerates an empty data dir — the registration page
    /// brings the first user to life.
    Serve {
        #[arg(long, default_value = "./data")]
        space_dir: PathBuf,
        #[arg(long, default_value = "127.0.0.1:7777")]
        listen: SocketAddr,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,hearth=debug")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init {
            space_dir,
            email,
            passphrase,
            owner,
        } => cmd_init(space_dir, email, passphrase, owner),
        Command::Serve { space_dir, listen } => cmd_serve(space_dir, listen),
    }
}

fn cmd_init(
    root: PathBuf,
    email: String,
    passphrase: Option<String>,
    owner: Option<String>,
) -> anyhow::Result<()> {
    let passphrase = match passphrase {
        Some(p) => p,
        None => {
            let p1 =
                rpassword::prompt_password("Choose a passphrase: ").context("read passphrase")?;
            let p2 = rpassword::prompt_password("Confirm: ").context("read passphrase confirm")?;
            if p1 != p2 {
                anyhow::bail!("passphrases do not match");
            }
            p1
        }
    };
    if passphrase.is_empty() {
        anyhow::bail!("passphrase must not be empty");
    }
    let normalised = normalise_email(&email).map_err(|e| anyhow::anyhow!("{e}"))?;
    let display_owner = owner
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .unwrap_or_else(|| normalised.clone());

    std::fs::create_dir_all(&root).context("create space-dir root")?;
    let mut registry = UsersRegistry::load(&root).map_err(|e| anyhow::anyhow!("{e}"))?;
    let entry = registry
        .add(&root, &normalised)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let space_dir = UsersRegistry::space_dir_for(&root, &entry.uuid);

    space::init::init_space(space::init::InitOptions {
        space_dir: space_dir.clone(),
        passphrase: SecretString::from(passphrase),
        owner: display_owner,
    })
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    println!(
        "Registered {} → {}\n  space at {}",
        entry.email,
        entry.uuid,
        space_dir.display()
    );
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn cmd_serve(space_dir: PathBuf, listen: SocketAddr) -> anyhow::Result<()> {
    // `serve` is allowed to start against an empty data root; the registration
    // page (POST /api/auth/init) brings it to life. Just make sure the
    // directory itself exists so the init handler can write into it.
    std::fs::create_dir_all(&space_dir).context("create space-dir")?;
    let state =
        AppState::new(space_dir, SessionStore::new()).map_err(|e| anyhow::anyhow!("{e}"))?;
    if !state.any_users() {
        tracing::info!("No users registered yet; serving the registration page only.");
    }
    let app = routes::build_router(state);

    tracing::info!("Listening on http://{listen}");
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .context("bind")?;
    axum::serve(listener, app).await.context("serve")?;
    Ok(())
}

// CLI is the only entrypoint. cmd_serve uses its own #[tokio::main] so cmd_init
// can run synchronously without spinning up a runtime.
