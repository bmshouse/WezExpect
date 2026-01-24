use anyhow::Result;
use clap::Parser as ClapParser;
use tokio::signal;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use wez_expect::{action, select_pane, Config, Monitor};

#[derive(ClapParser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[clap(short, long, default_value = "config.toml")]
    config: String,

    /// Override pane ID from config
    #[clap(short, long)]
    pane_id: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("Wez Expect - WezTerm Automation");
    info!("Starting up...");

    // Parse command line arguments
    let args = Args::parse();

    // Load configuration
    let config = if std::path::Path::new(&args.config).exists() {
        Config::load(&args.config)?
    } else {
        warn!("Config file '{}' not found, using defaults", args.config);
        Config::load_or_default()?
    };

    info!("Configuration loaded successfully");

    // Select pane (from CLI arg, config, or auto-select)
    let pane_id = match args.pane_id {
        Some(id) => {
            info!("Using pane ID from command line: {}", id);
            id
        }
        None => select_pane(config.pane.pane_id).await?,
    };

    // Create action factory
    let factory = std::sync::Arc::new(action::factory::BuiltinActionFactory::new());

    // Create monitor
    let monitor = Monitor::new(pane_id, config.rules.clone(), factory);

    info!(
        "Monitoring pane {} every {} seconds",
        pane_id, config.monitor.poll_interval_secs
    );
    info!("Press Ctrl+C to stop");

    // Run the main loop with graceful shutdown
    let result = tokio::select! {
        result = run_monitoring_loop(monitor, &config) => result,
        _ = wait_for_shutdown_signal() => {
            info!("Received shutdown signal, exiting gracefully");
            Ok(())
        }
    };

    match result {
        Ok(_) => info!("Wez Expect stopped"),
        Err(e) => error!("Wez Expect encountered an error: {}", e),
    }

    Ok(())
}

/// Main monitoring loop
async fn run_monitoring_loop(monitor: Monitor, config: &Config) -> Result<()> {
    let poll_interval = Duration::from_secs(config.monitor.poll_interval_secs);

    loop {
        // 1. Poll terminal for pattern matches
        tracing::debug!("Checking for matching patterns...");

        let match_result = loop {
            match monitor.check_for_timeout().await {
                Ok(Some(result)) => break result,
                Ok(None) => {
                    // No pattern matched, sleep and continue
                    sleep(poll_interval).await;
                    continue;
                }
                Err(e) => {
                    // Error checking pane (might be closed)
                    warn!(
                        "Error checking pane: {}. Retrying in {}s...",
                        e,
                        poll_interval.as_secs()
                    );
                    sleep(poll_interval).await;
                    continue;
                }
            }
        };

        // 2. Execute the action using the trait system
        let pane_id = monitor.pane_id();
        let rule_name = match_result.rule.name.as_deref().unwrap_or("unnamed");

        info!("Executing action for rule '{}'", rule_name);

        // Execute the action
        if let Err(e) = match_result
            .action
            .execute(pane_id, match_result.match_data, &match_result.rule.action)
            .await
        {
            error!("Failed to execute action: {}", e);
            sleep(poll_interval).await;
            continue;
        }

        // Post-execute hook (e.g., for immediate action content change detection)
        if let Err(e) = match_result.action.post_execute(pane_id).await {
            error!("Error in post-execute: {}", e);
        }

        // Wait for terminal content to change to prevent re-matching the same text
        // This is critical for all action types to avoid infinite loops
        tracing::debug!("Waiting for terminal content to change...");
        if let Err(e) = monitor.wait_for_content_change(poll_interval).await {
            error!("Error waiting for content change: {}", e);
            sleep(poll_interval).await;
        }

        // 3. Continue monitoring for next match
        tracing::debug!("Resuming monitoring for next pattern match...");
        sleep(poll_interval).await;
    }
}

/// Wait for Ctrl+C or other shutdown signal
async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
