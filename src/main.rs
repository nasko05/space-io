use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};

use hearth::routes;
use hearth::space::rate_limit::RateLimiter;
use hearth::space::session::SessionStore;
use hearth::state::{AppConfig, AppState};

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
    /// Serve over HTTP. Tolerates an empty data dir; the registration page
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
        Command::Serve { space_dir, listen } => cmd_serve(space_dir, listen),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn cmd_serve(space_dir: PathBuf, listen: SocketAddr) -> anyhow::Result<()> {
    std::fs::create_dir_all(&space_dir).context("create space-dir")?;

    let sessions = SessionStore::new();
    let unlock_limiter = RateLimiter::new();
    let config = AppConfig::from_env();
    if !config.cookie_secure {
        tracing::warn!(
            "HEARTH_INSECURE_COOKIES=1: session cookies will not be marked Secure. \
             Acceptable for localhost dev only — any production deploy should run \
             behind TLS and leave this unset."
        );
    }
    let state = AppState::new(space_dir, sessions.clone(), unlock_limiter.clone(), config)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if !state.any_users() {
        tracing::info!("No users registered yet; serving the registration page only.");
    }

    let sweep_sessions = sessions.clone();
    let sweep_limiter = unlock_limiter.clone();
    tokio::spawn(async move {
        const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);
        let mut tick = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            tick.tick().await;
            sweep_sessions.sweep_expired();
            sweep_limiter.sweep();
        }
    });

    let app = routes::build_router(state);

    tracing::info!("Listening on http://{listen}");
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .context("bind")?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("serve")?;
    Ok(())
}
