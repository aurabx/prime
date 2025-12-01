use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub dicom: DicomConfig,
    pub storage: StorageConfig,
    pub jmix: JmixConfig,
    pub http: HttpConfig,
    pub database: DatabaseConfig,
    #[serde(default = "default_authorized_aes")]
    pub authorized_aes: Vec<AuthorizedAEConfig>,
    
    /// Whether to reject connections from AEs not in authorized_aes
    /// If true, only AEs explicitly listed in authorized_aes will be accepted
    /// If false (default), all AEs are accepted
    #[serde(default = "default_strict_ae_auth")]
    pub strict_ae_auth: bool,
}

fn default_authorized_aes() -> Vec<AuthorizedAEConfig> {
    Vec::new()
}

fn default_strict_ae_auth() -> bool {
    false
}

#[derive(Clone, Debug, Deserialize)]
pub struct DicomConfig {
    pub bind_addr: String,
    pub port: u16,
    pub ae_title: String,
    pub max_associations: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StorageConfig {
    pub base_path: PathBuf,
    pub jmix_dir: String,
    pub dicom_temp_dir: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JmixConfig {
    pub skip_hashing: bool,
    pub skip_listing: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HttpConfig {
    pub bind_addr: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizedAEConfig {
    pub ae_title: String,
    pub description: Option<String>,
    pub allowed_ips: Option<Vec<String>>,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .context(format!("Failed to read config file: {:?}", path.as_ref()))?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config TOML")?;
            
        Ok(config)
    }
    
    pub fn from_str(content: &str) -> Result<Self> {
        let config: Config = toml::from_str(content)
            .context("Failed to parse config TOML")?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG: &str = r#"
[dicom]
bind_addr = "127.0.0.1"
port = 11112
ae_title = "TEST_SCP"
max_associations = 5

[storage]
base_path = "./test_data"
jmix_dir = "jmix"
dicom_temp_dir = "temp"

[jmix]
skip_hashing = true
skip_listing = false

[http]
bind_addr = "0.0.0.0"
port = 8080

[database]
path = "./test_data/test.redb"

[[authorized_aes]]
ae_title = "TEST_SCP"
description = "Test SCP for unit testing"
allowed_ips = ["127.0.0.1"]

[[authorized_aes]]
ae_title = "ANOTHER_SCP"
description = "Another test SCP"
"#;

    #[test]
    fn test_parse_valid_config() {
        let config = Config::from_str(VALID_CONFIG).unwrap();
        
        assert!(!config.strict_ae_auth); // Should default to false
        
        assert_eq!(config.dicom.bind_addr, "127.0.0.1");
        assert_eq!(config.dicom.port, 11112);
        assert_eq!(config.dicom.ae_title, "TEST_SCP");
        assert_eq!(config.dicom.max_associations, 5);
        
        assert_eq!(config.storage.base_path, PathBuf::from("./test_data"));
        assert_eq!(config.storage.jmix_dir, "jmix");
        assert_eq!(config.storage.dicom_temp_dir, "temp");
        
        assert!(config.jmix.skip_hashing);
        assert!(!config.jmix.skip_listing);
        
        assert_eq!(config.http.bind_addr, "0.0.0.0");
        assert_eq!(config.http.port, 8080);
        
assert_eq!(config.database.path, PathBuf::from("./test_data/test.redb"));
        
        assert_eq!(config.authorized_aes.len(), 2);
        assert_eq!(config.authorized_aes[0].ae_title, "TEST_SCP");
        assert_eq!(config.authorized_aes[0].description.as_ref().unwrap(), &"Test SCP for unit testing");
        assert_eq!(config.authorized_aes[0].allowed_ips.as_ref().unwrap()[0], "127.0.0.1");
        assert_eq!(config.authorized_aes[1].ae_title, "ANOTHER_SCP");
        assert_eq!(config.authorized_aes[1].description.as_ref().unwrap(), &"Another test SCP");
    }

    #[test]
    fn test_parse_missing_section() {
        let incomplete = r#"
[dicom]
bind_addr = "127.0.0.1"
port = 11112
ae_title = "TEST"
max_associations = 5
"#;
        let result = Config::from_str(incomplete);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_port() {
        let invalid = r#"
[dicom]
bind_addr = "127.0.0.1"
port = "not_a_number"
ae_title = "TEST"
max_associations = 5

[storage]
base_path = "./data"
jmix_dir = "jmix"
dicom_temp_dir = "temp"

[jmix]
skip_hashing = false
skip_listing = false

[http]
bind_addr = "0.0.0.0"
port = 8080

[database]
path = "./data/test.redb"
"#;
        let result = Config::from_str(invalid);
        assert!(result.is_err());
    }
}
