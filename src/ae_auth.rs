//! Implementation for Application Entity (AE) authentication in DICOM

use dimse::config::DimseConfig;
use dimse::scp::{QueryProvider, DimseScp};
use dimse::types::DatasetStream;
use dimse::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tokio::task::block_in_place;
use tracing::{info, debug, warn};
use crate::config::AuthorizedAEConfig;

/// A wrapper around a QueryProvider that validates the calling AE title
pub struct AuthorizedQueryProvider {
    inner: Arc<dyn QueryProvider>,
    authorized_aes: Vec<AuthorizedAEConfig>,
    current_calling_ae: tokio::sync::RwLock<Option<String>>,
}

impl AuthorizedQueryProvider {
    pub fn new(query_provider: Arc<dyn QueryProvider>, authorized_aes: Vec<AuthorizedAEConfig>) -> Self {
        Self {
            inner: query_provider,
            authorized_aes,
            current_calling_ae: tokio::sync::RwLock::new(None),
        }
    }
    
    pub async fn set_calling_ae(&self, ae_title: String) -> bool {
        // Check if this AE is authorized
        let is_authorized = self.authorized_aes.iter().any(|ae| ae.ae_title == ae_title);
        
        if is_authorized {
            info!("AE '{}' is authorized", ae_title);
        } else {
            warn!("AE '{}' is not authorized", ae_title);
        }
        
        let mut current = self.current_calling_ae.write().await;
        *current = Some(ae_title);
        
        is_authorized
    }
    
    fn get_calling_ae(&self) -> Option<String> {
        // Note: This is a simplified approach. In a real DICOM association,
        // the calling AE title would be associated with the connection
        block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.current_calling_ae.read().await.clone()
            })
        })
    }
}

// Implement QueryProvider trait by delegating to the inner provider
// The actual AE validation would be done at the association level
#[async_trait::async_trait]
impl QueryProvider for AuthorizedQueryProvider {
    async fn find(&self, level: dimse::types::QueryLevel, params: &HashMap<String, String>, max: u32) -> Result<Vec<DatasetStream>> {
        self.inner.find(level, params, max).await
    }

    async fn locate(&self, level: dimse::types::QueryLevel, params: &HashMap<String, String>) -> Result<Vec<DatasetStream>> {
        self.inner.locate(level, params).await
    }

    async fn get(&self, level: dimse::types::QueryLevel, params: &HashMap<String, String>) -> Result<Vec<DatasetStream>> {
        self.inner.get(level, params).await
    }

    async fn store(&self, dataset: DatasetStream) -> Result<()> {
        let calling_ae = match self.get_calling_ae() {
            Some(ae) => ae,
            None => {
                // In this simplified implementation, we can't reliably get the calling AE
                // So we'll either reject or accept based on config
                warn!("Could not determine calling AE, rejecting store operation");
                return Err(dimse::error::DimseError::operation_failed(
                    "Unable to verify calling AE"
                ));
            }
        };
        
        // Validate the AE is authorized
        let is_authorized = self.authorized_aes.iter().any(|ae| ae.ae_title == calling_ae);
        
        if !is_authorized {
            warn!("Rejecting store operation from unauthorized AE: {}", calling_ae);
            return Err(dimse::error::DimseError::operation_failed(
                format!("AE '{}' is not authorized to store data", calling_ae)
            ));
        }
        
        debug!("Store operation from authorized AE '{}'", calling_ae);
        self.inner.store(dataset).await
    }
}

/// Simple DICOM SCP with AE authorization
pub async fn start_authorized_dicom_scp(
    config: crate::config::DicomConfig,
    store_handler: Arc<crate::store_handler::StoreHandler>,
    authorized_aes: Vec<AuthorizedAEConfig>,
    strict_auth: bool,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    // Convert configuration
    let bind_addr = config.bind_addr.parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address: {}", e))?;
    
    info!("Configuring authorized DICOM SCP...");
    info!("  AE Title: {}", config.ae_title);
    info!("  Bind Address: {}:{} (Strict Auth: {})", bind_addr, config.port, strict_auth);
    info!("  Max Associations: {}", config.max_associations);
    
    let mut dimse_config = DimseConfig::default();
    dimse_config.bind_addr = bind_addr;
    dimse_config.port = config.port;
    dimse_config.local_aet = config.ae_title.clone();
    dimse_config.max_associations = config.max_associations;
    dimse_config.enable_store = true;
    
    // Create authorized query provider
    let authorized_provider = Arc::new(AuthorizedQueryProvider::new(store_handler, authorized_aes));
    
    let scp = DimseScp::new(dimse_config, authorized_provider);
    
    info!("DICOM SCP with AE authorization listening on {}:{} (AET: {})", 
           bind_addr, config.port, config.ae_title);
    
    scp.run(shutdown).await.map_err(|e| anyhow::anyhow!("DICOM SCP error: {}", e))
}