mod config;
mod routes;

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// State shared by all handlers. Future upstream clients (e.g. a `reqwest::Client`) go here.
#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    pub config: Arc<Config>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port = config.port;
    let state = AppState {
        config: Arc::new(config),
    };

    let listener = bind_listener(port)?;
    tracing::info!("fetcherr listening on {}", listener.local_addr()?);

    axum::serve(listener, routes::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("fetcherr stopped");
    Ok(())
}

/// Binds a dual-stack `[::]` socket so the API answers on both IPv6 and IPv4.
/// `homeassistant.local` resolves to IPv6 first via mDNS, so IPv4-only listening is not enough.
/// Falls back to `0.0.0.0` when IPv6 is unavailable in the container.
fn bind_listener(port: u16) -> std::io::Result<TcpListener> {
    match bind_socket(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)), true) {
        Ok(listener) => Ok(listener),
        Err(err) => {
            tracing::warn!("IPv6 bind failed ({err}), falling back to IPv4 only");
            bind_socket(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)), false)
        }
    }
}

fn bind_socket(addr: SocketAddr, dual_stack: bool) -> std::io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if dual_stack {
        socket.set_only_v6(false)?;
    }
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    TcpListener::from_std(socket.into())
}

/// Resolves on SIGINT (Ctrl+C) or SIGTERM (sent by Docker / the HA Supervisor on stop).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
