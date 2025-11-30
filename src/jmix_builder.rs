use anyhow::{Context, Result};
use std::path::Path;
use std::fs;
use crate::config::JmixConfig;
use crate::storage::StorageManager;
use crate::database::{Database, JmixPackageRecord};
use jmix_rs::builder::JmixBuilder;
use std::sync::Arc;
use chrono::Utc;
use tracing::{info, error};

pub struct JmixCreator {
    storage: Arc<StorageManager>,
    database: Arc<Database>,
    config: JmixConfig,
}

impl JmixCreator {
    pub fn new(storage: Arc<StorageManager>, database: Arc<Database>, config: JmixConfig) -> Self {
        Self { storage, database, config }
    }

    pub async fn process_study(&self, study_uid: &str) -> Result<String> {
        let study_dir = self.storage.dicom_temp_dir_for_study(study_uid);
        if !study_dir.exists() {
            return Err(anyhow::anyhow!("Study directory not found: {:?}", study_dir));
        }

        info!("Building JMIX package for study: {}", study_uid);

        let builder = JmixBuilder::new();
        let mut jmix_config = jmix_rs::config::Config::default();
        jmix_config.sender.name = "Prime".to_string();
        jmix_config.sender.id = "org:prime".to_string();
        
        let (envelope, dicom_files) = builder
            .build_from_dicom_with_options(
                &study_dir,
                &jmix_config,
                self.config.skip_hashing,
                self.config.skip_listing,
            )
            .map_err(|e| anyhow::anyhow!("JMIX build error: {}", e))?;

        let jmix_id = envelope.manifest.id.clone();
        let pkg_dir = self.storage.jmix_package_dir(&jmix_id);
        
        fs::create_dir_all(&pkg_dir)
            .context(format!("Failed to create package dir: {:?}", pkg_dir))?;

        builder.save_to_files_with_options(
            &envelope,
            &dicom_files,
            &pkg_dir,
            self.config.skip_hashing,
            self.config.skip_listing,
        ).map_err(|e| anyhow::anyhow!("JMIX save error: {}", e))?;

        let zip_path = self.storage.jmix_package_zip(&jmix_id);
        if !zip_path.exists() {
            return Err(anyhow::anyhow!("JMIX zip file not created: {:?}", zip_path));
        }
        
        let metadata = fs::metadata(&zip_path)?;
        let file_size = metadata.len();

        let record = JmixPackageRecord {
            id: jmix_id.clone(),
            study_uid: study_uid.to_string(),
            created_at: Utc::now().timestamp(),
            file_size,
            file_path: pkg_dir.to_string_lossy().to_string(),
        };
        
        self.database.record_package(&record)?;
        self.storage.cleanup_temp_study(study_uid)?;

        info!("Created JMIX package: {} for study: {}", jmix_id, study_uid);
        Ok(jmix_id)
    }

    pub async fn scan_completed_studies(&self) -> Result<()> {
        let temp_dir = self.storage.temp_dir();
        
        let mut entries = tokio::fs::read_dir(temp_dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let study_uid = entry.file_name().to_string_lossy().to_string();
                
                if self.is_study_stable(&entry.path()).await? {
                    info!("Processing stable study: {}", study_uid);
                    if let Err(e) = self.process_study(&study_uid).await {
                        error!("Failed to process study {}: {}", study_uid, e);
                    }
                }
            }
        }
        Ok(())
    }

    async fn is_study_stable(&self, path: &Path) -> Result<bool> {
        let metadata = tokio::fs::metadata(path).await?;
        let modified = metadata.modified()?;
        let elapsed = modified.elapsed().unwrap_or(std::time::Duration::from_secs(0));
        Ok(elapsed.as_secs() > 10)
    }
}
