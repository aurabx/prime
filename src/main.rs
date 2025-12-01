use anyhow::Result;
use prime::config::Config;
use prime::storage::StorageManager;
use prime::database::Database;
use prime::store_handler::StoreHandler;
use prime::jmix_builder::JmixCreator;
use prime::http_api::{AppState, create_router};
use prime::ae_auth::start_authorized_dicom_scp;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Prime...");

    let config_path = std::env::args().nth(1).unwrap_or_else(|| "prime.toml".to_string());
    let config = Config::load(&config_path)?;
    
    info!("Configuration loaded from {}", config_path);
    info!("DICOM SCP: {}:{} (AET: {})", config.dicom.bind_addr, config.dicom.port, config.dicom.ae_title);
    info!("HTTP API: {}:{}", config.http.bind_addr, config.http.port);

    // Initialize storage
    let storage = Arc::new(StorageManager::new(&config.storage)?);
    info!("Storage initialized at {:?}", config.storage.base_path);

    // Initialize database
    let database = Arc::new(Database::open(&config.database.path)?);
    info!("Database opened at {:?}", config.database.path);

    // Create store handler for DICOM SCP
    let store_handler = Arc::new(StoreHandler::new(
        storage.clone(),
        database.clone(),
        config.jmix.clone(),
    ));

    // Create JMIX creator for background processing
    let jmix_creator = Arc::new(JmixCreator::new(
        storage.clone(),
        database.clone(),
        config.jmix.clone(),
    ));

    // Shutdown tokens
    let dicom_shutdown = CancellationToken::new();
    let http_shutdown = CancellationToken::new();
    let scanner_shutdown = CancellationToken::new();

    // Start DICOM SCP with AE authorization
    let dicom_config = config.dicom.clone();
    let authorized_aes = config.authorized_aes.clone();
    let strict_auth = config.strict_ae_auth;
    let dicom_shutdown_clone = dicom_shutdown.clone();
    let dicom_handle = tokio::spawn(async move {
        if let Err(e) = start_authorized_dicom_scp(dicom_config, store_handler, authorized_aes, strict_auth, dicom_shutdown_clone).await {
            tracing::error!("DICOM SCP error: {}", e);
        }
    });
    info!("DICOM SCP started");

    // Start HTTP API
    let http_config = config.http.clone();
    let app_state = AppState {
        storage: storage.clone(),
        database: database.clone(),
    };
    let http_shutdown_clone = http_shutdown.clone();
    let http_handle = tokio::spawn(async move {
        let app = create_router(app_state);
        let addr = format!("{}:{}", http_config.bind_addr, http_config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await.expect("Failed to bind HTTP");
        info!("HTTP API listening on {}", addr);
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { http_shutdown_clone.cancelled().await })
            .await
            .expect("HTTP server error");
    });

    // Start background scanner for completed studies
    let scanner_shutdown_clone = scanner_shutdown.clone();
    let scanner_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = jmix_creator.scan_completed_studies().await {
                        tracing::warn!("Scanner error: {}", e);
                    }
                }
                _ = scanner_shutdown_clone.cancelled() => {
                    break;
                }
            }
        }
    });
    info!("Background scanner started");

    info!("Prime is running. Press Ctrl+C to shutdown.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Trigger shutdown
    dicom_shutdown.cancel();
    http_shutdown.cancel();
    scanner_shutdown.cancel();

    // Wait for tasks
    let _ = tokio::join!(dicom_handle, http_handle, scanner_handle);

    info!("Prime shut down gracefully");
    Ok(())
}
