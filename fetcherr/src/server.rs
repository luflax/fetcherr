use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use tokio::net::TcpListener;

/// Binds a dual-stack `[::]` socket so the server answers on both IPv6 and IPv4.
/// `homeassistant.local` resolves to IPv6 first via mDNS, so IPv4-only listening is not enough.
pub fn bind_listener(port: u16) -> std::io::Result<TcpListener> {
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
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!("cannot listen for Ctrl+C: {err}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::error!("cannot listen for SIGTERM: {err}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
