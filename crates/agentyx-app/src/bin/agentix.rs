//! `agentix` CLI entrypoint.
//!
//! MVP scope: `agentix serve` starts the headless web/LAN server
//! with the same embedded Axum server used by the desktop app.

#![deny(unsafe_code)]
#![allow(dead_code)]
#![warn(missing_docs)]

use std::env;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[path = "../commands/mod.rs"]
mod commands;
#[path = "../events.rs"]
mod events;
#[path = "../server/mod.rs"]
mod server;
#[path = "../sink.rs"]
mod sink;
#[path = "../state.rs"]
mod state;

use server::ServerConfig;
use state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Serve(ServeOpts),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServeOpts {
    host: String,
    port: u16,
    require_token: bool,
}

impl Default for ServeOpts {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 18_765,
            require_token: false,
        }
    }
}

fn main() -> anyhow::Result<()> {
    init_tracing();

    match parse_args(env::args().skip(1))? {
        Command::Serve(opts) => run_serve(opts),
        Command::Help => {
            print_help()?;
            Ok(())
        }
    }
}

fn run_serve(opts: ServeOpts) -> anyhow::Result<()> {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %opts.host,
        port = opts.port,
        require_token = opts.require_token,
        "agentix serve starting"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;

    runtime.block_on(async move {
        let app_state = AppState::initialize().context("initializing AppState")?;
        if let Err(e) = app_state.recover_orphan_runs() {
            tracing::warn!(error = %e, "orphan run recovery failed; continuing");
        }

        let state = Arc::new(app_state);
        let server_state = server::lifecycle::build_state(state.clone());
        state.attach_server(server_state.clone());

        let config = ServerConfig {
            enabled: true,
            bind_host: opts.host,
            port: opts.port,
            lan_enabled: true,
            require_token: opts.require_token,
            rate_limit_per_window: 60,
            rate_window: Duration::from_secs(10),
        };

        let info = server::lifecycle::start(server_state, config)
            .await
            .context("starting embedded HTTP server")?;
        tracing::info!(bind = %info.bind_addr, "Agentyx web listening");

        std::future::pending::<()>().await;
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<Command> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(Command::Help);
    };

    match command.as_str() {
        "serve" => parse_serve_opts(args).map(Command::Serve),
        "--help" | "-h" | "help" => Ok(Command::Help),
        other => anyhow::bail!("unknown command: {other}"),
    }
}

fn parse_serve_opts(args: impl IntoIterator<Item = String>) -> anyhow::Result<ServeOpts> {
    let mut opts = ServeOpts::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => {
                opts.host = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--host requires a value"))?;
            }
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--port requires a value"))?;
                opts.port = value
                    .parse()
                    .with_context(|| format!("invalid --port value: {value}"))?;
            }
            "--require-token" => {
                opts.require_token = true;
            }
            "--help" | "-h" => {
                print_help()?;
                std::process::exit(0);
            }
            other => {
                anyhow::bail!("unknown serve argument: {other}");
            }
        }
    }
    Ok(opts)
}

fn print_help() -> anyhow::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(
        b"Usage: agentix serve [--host HOST] [--port PORT] [--require-token]\n\n\
Commands:\n  serve    Start the Agentyx web/LAN server\n",
    )?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("agentyx_app=info,agentyx_core=info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact())
        .try_init();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn serve_uses_lan_defaults() {
        let command = parse_args(["serve"].into_iter().map(str::to_string)).unwrap();
        assert_eq!(command, Command::Serve(ServeOpts::default()));
    }

    #[test]
    fn serve_accepts_host_port_and_token_flag() {
        let command = parse_args(
            [
                "serve",
                "--host",
                "127.0.0.1",
                "--port",
                "3000",
                "--require-token",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(
            command,
            Command::Serve(ServeOpts {
                host: "127.0.0.1".to_string(),
                port: 3000,
                require_token: true,
            })
        );
    }

    #[test]
    fn no_args_prints_help() {
        let command = parse_args(std::iter::empty()).unwrap();
        assert_eq!(command, Command::Help);
    }
}
