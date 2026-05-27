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
use crate::space::Space;
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
    /// Initialise a new space (creates .space.toml, seed note, first git commit).
    Init {
        #[arg(long, default_value = "./data")]
        space_dir: PathBuf,
        /// Passphrase. If omitted, prompted interactively.
        #[arg(long)]
        passphrase: Option<String>,
        /// Owner display name shown on the unlock screen.
        #[arg(long, default_value = "ada@home.lan")]
        owner: String,
    },
    /// Serve the space over HTTP.
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
            passphrase,
            owner,
        } => cmd_init(space_dir, passphrase, owner),
        Command::Serve { space_dir, listen } => cmd_serve(space_dir, listen),
    }
}

fn cmd_init(space_dir: PathBuf, passphrase: Option<String>, owner: String) -> anyhow::Result<()> {
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

    space::init::init_space(space::init::InitOptions {
        space_dir: space_dir.clone(),
        passphrase: SecretString::from(passphrase),
        owner,
    })
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    println!("Space initialised at {}", space_dir.display());
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn cmd_serve(space_dir: PathBuf, listen: SocketAddr) -> anyhow::Result<()> {
    let space = Space::open(space_dir).map_err(|e| anyhow::anyhow!("{e}"))?;
    let state = AppState {
        space,
        sessions: SessionStore::new(),
    };
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
