use chao_game_server::service::{AppState, Config, router, spawn_simulation};
use std::io::{self, BufRead};

async fn shutdown_signal() {
    let stdin_shutdown = if std::env::var("CHAO_SHUTDOWN_STDIN").as_deref() == Ok("1") {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        // A detached ordinary thread avoids Tokio's non-cancellable stdin read
        // delaying runtime shutdown when Ctrl+C arrives before any stdin data.
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(line) if line.trim() == "shutdown" => break,
                    Err(_) => break,
                    _ => {}
                }
            }
            // EOF also means the controlling launcher has gone away.
            let _ = sender.send(());
        });
        Some(receiver)
    } else {
        None
    };
    let controlling_launcher = async move {
        if let Some(receiver) = stdin_shutdown {
            let _ = receiver.await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    let operating_system = async {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    };
    tokio::select! { _ = controlling_launcher => {}, _ = operating_system => {} }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let config = Config::from_env().map_err(io::Error::other)?;
    let state = AppState::load(config)?;
    let listener = tokio::net::TcpListener::bind(state.config.address).await?;
    println!("Chao Rust game server: http://{}", listener.local_addr()?);
    println!("Data directory: {}", state.config.data_dir.display());
    let simulation = spawn_simulation(state.clone());
    let shutdown_state = state.clone();
    let serving = axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_state.shutdown.send(true);
        })
        .await;
    let _ = state.shutdown.send(true);
    let _ = simulation.await;
    serving
}
