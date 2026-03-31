use tephra_server::api;
use tephra_server::discovery;
use tephra_server::monitor;

use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use tephra_server::api::AppState;
use tephra_server::monitor::SystemState;

#[derive(Parser)]
#[command(name = "tephra", about = "Tephra — lightweight CPU thermal monitoring agent")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "9867")]
    port: u16,

    /// Sampling interval in milliseconds
    #[arg(short, long, default_value = "500")]
    interval: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let n_cores = monitor::get_cpu_count();
    let max_freq = monitor::get_max_freq();
    let hostname = monitor::get_hostname();
    let cpu_model = monitor::get_cpu_model();

    info!(
        "Starting tephra v{} on {} ({}, {} cores)",
        env!("CARGO_PKG_VERSION"),
        hostname,
        cpu_model,
        n_cores
    );

    let mut state = SystemState::new(n_cores, max_freq);
    state.sample_interval_secs = args.interval as f64 / 1000.0;

    let (sse_tx, _) = broadcast::channel(64);

    let app_state = Arc::new(AppState {
        system_state: RwLock::new(state),
        sse_tx,
        interval_ms: args.interval,
    });

    // Start sampling loop
    let sampling_state = Arc::clone(&app_state);
    tokio::spawn(api::sampling_loop(sampling_state));

    // Register mDNS
    let mdns = discovery::MdnsRegistration::register(&hostname, args.port, &cpu_model, n_cores);
    if mdns.is_none() {
        warn!("mDNS discovery is disabled — agents must be added manually");
    }

    // Start HTTP server
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let app = api::router(app_state);

    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Cleanup
    if let Some(m) = mdns {
        m.unregister();
    }

    info!("Shut down gracefully");
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");

    tokio::select! {
        _ = ctrl_c => { info!("Received SIGINT"); }
        _ = sigterm.recv() => { info!("Received SIGTERM"); }
    }
}

#[cfg(windows)]
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    info!("Received Ctrl+C");
}
