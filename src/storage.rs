use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};
use crate::config::StorageConfig;

#[derive(Clone)]
pub struct StorageManager {
    base_path: PathBuf,
    jmix_dir: PathBuf,
    dicom_temp_dir: PathBuf,
}

impl StorageManager {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let base_path = config.base_path.clone();
        let jmix_dir = base_path.join(&config.jmix_dir);
        let dicom_temp_dir = base_path.join(&config.dicom_temp_dir);
        
        let manager = Self {
            base_path,
            jmix_dir,
            dicom_temp_dir,
        };
        
        manager.ensure_dirs()?;
        
        Ok(manager)
    }
    
    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path).context("Failed to create base storage dir")?;
        fs::create_dir_all(&self.jmix_dir).context("Failed to create jmix storage dir")?;
        fs::create_dir_all(&self.dicom_temp_dir).context("Failed to create dicom temp storage dir")?;
        Ok(())
    }
    
    pub fn dicom_temp_dir_for_study(&self, study_uid: &str) -> PathBuf {
        self.dicom_temp_dir.join(study_uid)
    }
    
    pub fn jmix_package_dir(&self, jmix_id: &str) -> PathBuf {
        self.jmix_dir.join(jmix_id)
    }
    
    pub fn jmix_package_zip(&self, jmix_id: &str) -> PathBuf {
        self.jmix_package_dir(jmix_id).join(format!("{}.zip", jmix_id))
    }
    
    pub fn ensure_study_temp_dir(&self, study_uid: &str) -> Result<PathBuf> {
        let dir = self.dicom_temp_dir_for_study(study_uid);
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }
    
    pub fn cleanup_temp_study(&self, study_uid: &str) -> Result<()> {
        let dir = self.dicom_temp_dir_for_study(study_uid);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
    
    pub fn temp_dir(&self) -> &Path {
        &self.dicom_temp_dir
    }
    
    pub fn jmix_dir(&self) -> &Path {
        &self.jmix_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::config::StorageConfig;

    fn create_test_config(base: &Path) -> StorageConfig {
        StorageConfig {
            base_path: base.to_path_buf(),
            jmix_dir: "jmix-packages".to_string(),
            dicom_temp_dir: "dicom-temp".to_string(),
        }
    }

    #[test]
    fn test_storage_manager_creates_directories() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        
        let _manager = StorageManager::new(&config).unwrap();
        
        assert!(temp.path().join("jmix-packages").exists());
        assert!(temp.path().join("dicom-temp").exists());
    }

    #[test]
    fn test_dicom_temp_dir_for_study() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        let manager = StorageManager::new(&config).unwrap();
        
        let study_dir = manager.dicom_temp_dir_for_study("1.2.3.4.5");
        assert_eq!(study_dir, temp.path().join("dicom-temp").join("1.2.3.4.5"));
    }

    #[test]
    fn test_jmix_package_paths() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        let manager = StorageManager::new(&config).unwrap();
        
        let pkg_dir = manager.jmix_package_dir("test-id");
        assert_eq!(pkg_dir, temp.path().join("jmix-packages").join("test-id"));
        
        let zip_path = manager.jmix_package_zip("test-id");
        assert_eq!(zip_path, temp.path().join("jmix-packages").join("test-id").join("test-id.zip"));
    }

    #[test]
    fn test_ensure_study_temp_dir() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        let manager = StorageManager::new(&config).unwrap();
        
        let study_dir = manager.ensure_study_temp_dir("1.2.3.4.5").unwrap();
        assert!(study_dir.exists());
        assert!(study_dir.is_dir());
    }

    #[test]
    fn test_cleanup_temp_study() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        let manager = StorageManager::new(&config).unwrap();
        
        // Create a study dir with a file
        let study_dir = manager.ensure_study_temp_dir("1.2.3.4.5").unwrap();
        fs::write(study_dir.join("test.dcm"), b"test data").unwrap();
        assert!(study_dir.exists());
        
        // Cleanup
        manager.cleanup_temp_study("1.2.3.4.5").unwrap();
        assert!(!study_dir.exists());
    }

    #[test]
    fn test_cleanup_nonexistent_study() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(temp.path());
        let manager = StorageManager::new(&config).unwrap();
        
        // Should not error when cleaning up non-existent study
        let result = manager.cleanup_temp_study("nonexistent");
        assert!(result.is_ok());
    }
}
