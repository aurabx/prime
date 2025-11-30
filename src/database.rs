use std::path::Path;
use redb::{Database as RedbDatabase, TableDefinition, ReadableTable};
use serde::{Serialize, Deserialize};
use anyhow::{Context, Result};
use std::sync::Arc;

const JMIX_PACKAGES: TableDefinition<&str, &str> = TableDefinition::new("jmix_packages");
const STUDY_INDEX: TableDefinition<&str, &str> = TableDefinition::new("study_index");

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JmixPackageRecord {
    pub id: String,
    pub study_uid: String,
    pub created_at: i64,
    pub file_size: u64,
    pub file_path: String,
}

#[derive(Clone)]
pub struct Database {
    db: Arc<RedbDatabase>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = RedbDatabase::create(path.as_ref())
            .context("Failed to create/open database")?;
            
        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(JMIX_PACKAGES)?;
            let _ = write_txn.open_table(STUDY_INDEX)?;
        }
        write_txn.commit()?;
        
        Ok(Self {
            db: Arc::new(db),
        })
    }
    
    pub fn record_package(&self, pkg: &JmixPackageRecord) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut pkg_table = write_txn.open_table(JMIX_PACKAGES)?;
            let mut study_table = write_txn.open_table(STUDY_INDEX)?;
            
            // Save package record
            let json = serde_json::to_string(pkg)?;
            pkg_table.insert(pkg.id.as_str(), json.as_str())?;
            
            // Update study index
            // We append to existing list of IDs for this study
            let mut ids = if let Some(existing) = study_table.get(pkg.study_uid.as_str())? {
                let existing_str = existing.value();
                serde_json::from_str::<Vec<String>>(existing_str).unwrap_or_default()
            } else {
                Vec::new()
            };
            
            if !ids.contains(&pkg.id) {
                ids.push(pkg.id.clone());
                let ids_json = serde_json::to_string(&ids)?;
                study_table.insert(pkg.study_uid.as_str(), ids_json.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }
    
    pub fn get_package(&self, id: &str) -> Result<Option<JmixPackageRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(JMIX_PACKAGES)?;
        
        if let Some(record) = table.get(id)? {
            let pkg: JmixPackageRecord = serde_json::from_str(record.value())?;
            Ok(Some(pkg))
        } else {
            Ok(None)
        }
    }
    
    pub fn get_packages_by_study(&self, study_uid: &str) -> Result<Vec<JmixPackageRecord>> {
        let read_txn = self.db.begin_read()?;
        let study_table = read_txn.open_table(STUDY_INDEX)?;
        let pkg_table = read_txn.open_table(JMIX_PACKAGES)?;
        
        let mut results = Vec::new();
        
        if let Some(ids_record) = study_table.get(study_uid)? {
            let ids: Vec<String> = serde_json::from_str(ids_record.value())?;
            for id in ids {
                if let Some(pkg_record) = pkg_table.get(id.as_str())? {
                    if let Ok(pkg) = serde_json::from_str::<JmixPackageRecord>(pkg_record.value()) {
                        results.push(pkg);
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    pub fn list_packages(&self, limit: usize, offset: usize) -> Result<Vec<JmixPackageRecord>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(JMIX_PACKAGES)?;
        
        let mut results = Vec::new();
        let mut iter = table.iter()?;
        
        // This is not efficient pagination but fine for redb iteration
        let mut count = 0;
        let mut skipped = 0;
        
        while let Some(item) = iter.next() {
            if skipped < offset {
                skipped += 1;
                continue;
            }
            
            if count >= limit {
                break;
            }
            
            let (_, value) = item?;
            if let Ok(pkg) = serde_json::from_str::<JmixPackageRecord>(value.value()) {
                results.push(pkg);
                count += 1;
            }
        }
        
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_record(id: &str, study_uid: &str) -> JmixPackageRecord {
        JmixPackageRecord {
            id: id.to_string(),
            study_uid: study_uid.to_string(),
            created_at: 1234567890,
            file_size: 1024,
            file_path: format!("/test/{}", id),
        }
    }

    #[test]
    fn test_database_open_and_close() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        
        let _db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn test_record_and_get_package() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        let db = Database::open(&db_path).unwrap();
        
        let record = create_test_record("pkg-001", "1.2.3.4.5");
        db.record_package(&record).unwrap();
        
        let retrieved = db.get_package("pkg-001").unwrap();
        assert!(retrieved.is_some());
        
        let pkg = retrieved.unwrap();
        assert_eq!(pkg.id, "pkg-001");
        assert_eq!(pkg.study_uid, "1.2.3.4.5");
        assert_eq!(pkg.file_size, 1024);
    }

    #[test]
    fn test_get_nonexistent_package() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        let db = Database::open(&db_path).unwrap();
        
        let result = db.get_package("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_packages_by_study() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        let db = Database::open(&db_path).unwrap();
        
        // Add multiple packages for same study
        db.record_package(&create_test_record("pkg-001", "study-A")).unwrap();
        db.record_package(&create_test_record("pkg-002", "study-A")).unwrap();
        db.record_package(&create_test_record("pkg-003", "study-B")).unwrap();
        
        let study_a_packages = db.get_packages_by_study("study-A").unwrap();
        assert_eq!(study_a_packages.len(), 2);
        
        let study_b_packages = db.get_packages_by_study("study-B").unwrap();
        assert_eq!(study_b_packages.len(), 1);
        
        let study_c_packages = db.get_packages_by_study("study-C").unwrap();
        assert_eq!(study_c_packages.len(), 0);
    }

    #[test]
    fn test_list_packages_with_pagination() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        let db = Database::open(&db_path).unwrap();
        
        // Add 5 packages
        for i in 0..5 {
            db.record_package(&create_test_record(&format!("pkg-{:03}", i), "study")).unwrap();
        }
        
        // Test limit
        let first_two = db.list_packages(2, 0).unwrap();
        assert_eq!(first_two.len(), 2);
        
        // Test offset
        let all = db.list_packages(10, 0).unwrap();
        assert_eq!(all.len(), 5);
        
        let with_offset = db.list_packages(10, 3).unwrap();
        assert_eq!(with_offset.len(), 2);
    }

    #[test]
    fn test_duplicate_package_id() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.redb");
        let db = Database::open(&db_path).unwrap();
        
        let record1 = create_test_record("pkg-001", "study-A");
        let mut record2 = create_test_record("pkg-001", "study-B");
        record2.file_size = 2048;
        
        db.record_package(&record1).unwrap();
        db.record_package(&record2).unwrap();
        
        // Second record should overwrite
        let pkg = db.get_package("pkg-001").unwrap().unwrap();
        assert_eq!(pkg.study_uid, "study-B");
        assert_eq!(pkg.file_size, 2048);
    }
}
