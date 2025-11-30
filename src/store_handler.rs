use async_trait::async_trait;
use dimse::scp::QueryProvider;
use dimse::types::{DatasetStream, QueryLevel};
use dimse::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, debug, warn};
use crate::storage::StorageManager;
use crate::database::Database;
use crate::config::JmixConfig;
use dicom_object::meta::FileMetaTableBuilder;
use dicom_dictionary_std::uids;

#[allow(dead_code)]
pub struct StoreHandler {
    storage: Arc<StorageManager>,
    database: Arc<Database>,
    config: JmixConfig,
}

impl StoreHandler {
    pub fn new(storage: Arc<StorageManager>, database: Arc<Database>, config: JmixConfig) -> Self {
        Self { storage, database, config }
    }
}

#[async_trait]
impl QueryProvider for StoreHandler {
    async fn find(&self, _level: QueryLevel, _params: &HashMap<String, String>, _max: u32) -> Result<Vec<DatasetStream>> {
        Ok(Vec::new())
    }

    async fn locate(&self, _level: QueryLevel, _params: &HashMap<String, String>) -> Result<Vec<DatasetStream>> {
        Ok(Vec::new())
    }

    async fn get(&self, _level: QueryLevel, _params: &HashMap<String, String>) -> Result<Vec<DatasetStream>> {
        Ok(Vec::new())
    }

    async fn store(&self, dataset: DatasetStream) -> Result<()> {
        let sop_instance = dataset.metadata().sop_instance_uid.clone()
            .unwrap_or_else(|| "unknown".to_string());
        
        debug!("C-STORE received: SOP Instance {}", sop_instance);
        
        // Extract study UID from metadata
        let study_uid = dataset.metadata().study_instance_uid.clone()
            .or_else(|| {
                // Try to get from object if available
                if let DatasetStream::Object { object, .. } = &dataset {
                    object.element_by_name("StudyInstanceUID").ok()
                        .and_then(|e| e.to_str().ok().map(|s| s.to_string()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                warn!("No StudyInstanceUID found, using 'unknown_study'");
                "unknown_study".to_string()
            });
        
        info!("Storing DICOM instance for study: {}", study_uid);
        
        let study_dir = self.storage.ensure_study_temp_dir(&study_uid)
            .map_err(|e| dimse::error::DimseError::internal(e.to_string()))?;

        match &dataset {
             DatasetStream::Object { object, metadata } => {
                 let file_path = study_dir.join(format!("{}.dcm", metadata.id));
                 let sop_class = metadata.sop_class_uid.clone().unwrap_or_else(|| uids::SECONDARY_CAPTURE_IMAGE_STORAGE.to_string());
                 let sop_instance = metadata.sop_instance_uid.clone().unwrap_or_else(|| metadata.id.to_string());
                 let ts = metadata.transfer_syntax.clone().unwrap_or_else(|| uids::EXPLICIT_VR_LITTLE_ENDIAN.to_string());
                 
                 let file_meta = FileMetaTableBuilder::new()
                     .media_storage_sop_class_uid(&sop_class)
                     .media_storage_sop_instance_uid(&sop_instance)
                     .transfer_syntax(&ts);
                 
                 let file_obj = object.clone().with_meta(file_meta)
                     .map_err(|e| dimse::error::DimseError::internal(format!("Failed to create file object: {}", e)))?;
                 file_obj.write_to_file(&file_path)
                     .map_err(|e| dimse::error::DimseError::internal(format!("Failed to write DICOM: {}", e)))?;
                 debug!("Wrote DICOM file: {:?}", file_path);
             },
             _ => {
                 let path = dataset.to_temp_file(&study_dir).await
                     .map_err(|e| dimse::error::DimseError::internal(e.to_string()))?;
                 debug!("Wrote DICOM file: {:?}", path);
             }
        }

        info!("C-STORE completed for study: {}", study_uid);
        Ok(())
    }
}
