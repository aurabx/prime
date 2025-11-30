use dimse::config::DimseConfig;
use dimse::scp::DimseScp;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use anyhow::Result;
use tracing::{info, debug};
use crate::config::DicomConfig;
use crate::store_handler::StoreHandler;

pub async fn start_dicom_scp(
    config: DicomConfig,
    store_handler: Arc<StoreHandler>,
    shutdown: CancellationToken,
) -> Result<()> {
    let bind_addr = config.bind_addr.parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address: {}", e))?;

    info!("Configuring DICOM SCP...");
    info!("  AE Title: {}", config.ae_title);
    info!("  Bind Address: {}:{}", bind_addr, config.port);
    info!("  Max Associations: {}", config.max_associations);

    let mut dimse_config = DimseConfig::default();
    dimse_config.bind_addr = bind_addr;
    dimse_config.port = config.port;
    dimse_config.local_aet = config.ae_title.clone();
    dimse_config.max_associations = config.max_associations;
    dimse_config.enable_store = true;
    
    debug!("DIMSE config: enable_echo={}, enable_find={}, enable_move={}, enable_store={}",
        dimse_config.enable_echo, dimse_config.enable_find, dimse_config.enable_move, dimse_config.enable_store);
    
    let scp = DimseScp::new(dimse_config, store_handler);
    
    info!("DICOM SCP listening on {}:{} (AET: {})", bind_addr, config.port, config.ae_title);
    
    scp.run(shutdown).await.map_err(|e| anyhow::anyhow!("DICOM SCP error: {}", e))
}
