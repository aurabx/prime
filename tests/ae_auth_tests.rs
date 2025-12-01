//! Tests for AE authorization functionality

use prime::config::{AuthorizedAEConfig, Config, DicomConfig, StorageConfig, JmixConfig, HttpConfig, DatabaseConfig};
use prime::store_handler::StoreHandler;
use prime::storage::StorageManager;
use prime::database::Database;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_config() -> Config {
    let temp_dir = TempDir::new().unwrap();
    
    Config {
        dicom: DicomConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 11112,
            ae_title: "TEST_SCP".to_string(),
            max_associations: 5,
        },
        storage: StorageConfig {
            base_path: temp_dir.path().to_path_buf(),
            jmix_dir: "jmix".to_string(),
            dicom_temp_dir: "temp".to_string(),
        },
        jmix: JmixConfig {
            skip_hashing: true,
            skip_listing: false,
        },
        http: HttpConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 8080,
        },
        database: DatabaseConfig {
            path: temp_dir.path().join("test.redb"),
        },
        authorized_aes: vec![
            AuthorizedAEConfig {
                ae_title: "AUTHORIZED_AE_1".to_string(),
                description: Some("First authorized AE".to_string()),
                allowed_ips: Some(vec!["127.0.0.1".to_string()]),
            },
            AuthorizedAEConfig {
                ae_title: "AUTHORIZED_AE_2".to_string(),
                description: Some("Second authorized AE".to_string()),
                allowed_ips: None,
            },
        ],
        strict_ae_auth: true,
    }
}

fn create_test_ae_config() -> AuthorizedAEConfig {
    AuthorizedAEConfig {
        ae_title: "TEST_AE".to_string(),
        description: Some("Test AE for unit tests".to_string()),
        allowed_ips: Some(vec!["127.0.0.1".to_string()]),
    }
}

#[test]
fn test_config_ae_validation() {
    let config = create_test_config();
    
    // Test that the config has the expected AEs
    assert_eq!(config.authorized_aes.len(), 2);
    assert_eq!(config.authorized_aes[0].ae_title, "AUTHORIZED_AE_1");
    assert_eq!(config.authorized_aes[1].ae_title, "AUTHORIZED_AE_2");
    
    // Test that strict auth is enabled
    assert!(config.strict_ae_auth);
    
    // Test optional fields
    assert_eq!(config.authorized_aes[0].description.as_ref().unwrap(), "First authorized AE");
    assert_eq!(config.authorized_aes[0].allowed_ips.as_ref().unwrap()[0], "127.0.0.1");
    assert!(config.authorized_aes[1].allowed_ips.is_none());
}

#[test]
fn test_config_toml_parsing() {
    let toml_content = r#"
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
bind_addr = "127.0.0.1"
port = 8080

[database]
path = "./test_data/test.redb"

strict_ae_auth = false

[[authorized_aes]]
ae_title = "TOML_TEST_AE"
description = "Test AE from TOML"
allowed_ips = ["192.168.1.100"]

[[authorized_aes]]
ae_title = "UNRESTRICTED_AE"
description = "AE without IP restrictions"
"#;
    
    let config = Config::from_str(toml_content).unwrap();
    
    // Verify the TOML was parsed correctly
    assert!(!config.strict_ae_auth); // Changed to false
    assert_eq!(config.authorized_aes.len(), 2);
    assert_eq!(config.authorized_aes[0].ae_title, "TOML_TEST_AE");
    assert_eq!(config.authorized_aes[1].ae_title, "UNRESTRICTED_AE");
    assert_eq!(config.authorized_aes[0].allowed_ips.as_ref().unwrap()[0], "192.168.1.100");
    assert!(config.authorized_aes[1].allowed_ips.is_none());
}

#[test]
fn test_ae_authorization_check() {
    let aes = vec![
        AuthorizedAEConfig {
            ae_title: "AUTHORIZED".to_string(),
            description: None,
            allowed_ips: None,
        },
        AuthorizedAEConfig {
            ae_title: "ANOTHER_AUTH".to_string(),
            description: None,
            allowed_ips: None,
        },
    ];
    
    // Check authorized AEs
    assert!(aes.iter().any(|ae| ae.ae_title == "AUTHORIZED"));
    assert!(aes.iter().any(|ae| ae.ae_title == "ANOTHER_AUTH"));
    
    // Check non-authorized AE
    assert!(!aes.iter().any(|ae| ae.ae_title == "UNAUTHORIZED"));
}

#[test]
fn test_ae_with_multiple_ips() {
    let ae = AuthorizedAEConfig {
        ae_title: "MULTI_IP_AE".to_string(),
        description: Some("AE with multiple IP restrictions".to_string()),
        allowed_ips: Some(vec![
            "192.168.1.100".to_string(),
            "192.168.1.101".to_string(),
            "10.0.0.50".to_string(),
        ]),
    };
    
    assert_eq!(ae.ae_title, "MULTI_IP_AE");
    let ips = ae.allowed_ips.as_ref().unwrap();
    assert_eq!(ips.len(), 3);
    assert!(ips.contains(&"192.168.1.100".to_string()));
    assert!(ips.contains(&"192.168.1.101".to_string()));
    assert!(ips.contains(&"10.0.0.50".to_string()));
    assert!(!ips.contains(&"192.168.1.102".to_string()));
}

#[test]
fn test_empty_authorized_aes() {
    // Create a config directly instead of using TOML parsing
    let config = Config {
        dicom: DicomConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 11112,
            ae_title: "TEST_SCP".to_string(),
            max_associations: 5,
        },
        storage: StorageConfig {
            base_path: PathBuf::from("./test_data"),
            jmix_dir: "jmix".to_string(),
            dicom_temp_dir: "temp".to_string(),
        },
        jmix: JmixConfig {
            skip_hashing: true,
            skip_listing: false,
        },
        http: HttpConfig {
            bind_addr: "127.0.0.1".to_string(),
            port: 8080,
        },
        database: DatabaseConfig {
            path: PathBuf::from("./test_data/test.redb"),
        },
        authorized_aes: Vec::new(), // Empty authorized AEs
        strict_ae_auth: true, // Strict auth enabled
    };
    
    assert!(config.authorized_aes.is_empty());
    assert!(config.strict_ae_auth);
}

#[test]
fn test_strict_ae_auth_default_false() {
    let config_str = r#"
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
bind_addr = "127.0.0.1"
port = 8080

[database]
path = "./test_data/test.redb"
"#;
    
    let config = Config::from_str(config_str).unwrap();
    // strict_ae_auth should default to false
    assert!(!config.strict_ae_auth);
}

#[test]
fn test_large_number_of_aes() {
    let mut aes = Vec::new();
    
    // Create 100 authorized AEs
    for i in 0..100 {
        aes.push(AuthorizedAEConfig {
            ae_title: format!("AE_{:03}", i),
            description: Some(format!("Test AE number {}", i)),
            allowed_ips: if i % 2 == 0 {
                Some(vec![format!("192.168.{}.{}", i / 256, i % 256)])
            } else {
                None
            },
        });
    }
    
    assert_eq!(aes.len(), 100);
    
    // Verify we can check authorization for each
    assert!(aes.iter().any(|ae| ae.ae_title == "AE_000"));
    assert!(aes.iter().any(|ae| ae.ae_title == "AE_050"));
    assert!(aes.iter().any(|ae| ae.ae_title == "AE_099"));
    
    // Verify non-existent AE is not found
    assert!(!aes.iter().any(|ae| ae.ae_title == "AE_100"));
    assert!(!aes.iter().any(|ae| ae.ae_title == "NONEXISTENT"));
}

#[test]
fn test_ae_title_case_sensitivity() {
    let aes = vec![
        AuthorizedAEConfig {
            ae_title: "TestAE".to_string(),
            description: None,
            allowed_ips: None,
        },
    ];
    
    // AE titles should be case-sensitive
    assert!(aes.iter().any(|ae| ae.ae_title == "TestAE"));
    assert!(!aes.iter().any(|ae| ae.ae_title == "testae"));
    assert!(!aes.iter().any(|ae| ae.ae_title == "TESTAE"));
}

#[test]
fn test_ae_config_with_special_characters() {
    let ae = AuthorizedAEConfig {
        ae_title: "AE-WITH_SPECIAL.CHARS".to_string(),
        description: Some("Description with special chars: !@#$%^&*()".to_string()),
        allowed_ips: None,
    };
    
    assert_eq!(ae.ae_title, "AE-WITH_SPECIAL.CHARS");
    assert_eq!(ae.description.as_ref().unwrap(), "Description with special chars: !@#$%^&*()");
}

#[test]
fn test_multiple_ae_with_same_ip() {
    let aes = vec![
        AuthorizedAEConfig {
            ae_title: "AE1".to_string(),
            description: None,
            allowed_ips: Some(vec!["192.168.1.100".to_string()]),
        },
        AuthorizedAEConfig {
            ae_title: "AE2".to_string(),
            description: None,
            allowed_ips: Some(vec!["192.168.1.100".to_string()]),
        },
    ];
    
    // Both AEs should have the same allowed IP
    let ae1_ips = aes[0].allowed_ips.as_ref().unwrap();
    let ae2_ips = aes[1].allowed_ips.as_ref().unwrap();
    
    assert_eq!(ae1_ips[0], ae2_ips[0]);
    assert_eq!(ae1_ips[0], "192.168.1.100");
}

#[tokio::test]
async fn test_authorized_query_provider_authorization() {
    use prime::ae_auth::AuthorizedQueryProvider;
    
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(StorageManager::new(&StorageConfig {
        base_path: temp_dir.path().to_path_buf(),
        jmix_dir: "jmix".to_string(),
        dicom_temp_dir: "temp".to_string(),
    }).unwrap());
    
    let database = Arc::new(Database::open(&temp_dir.path().join("test.redb")).unwrap());
    let store_handler = Arc::new(StoreHandler::new(
        storage,
        database,
        JmixConfig {
            skip_hashing: true,
            skip_listing: false,
        }
    ));
    
    let authorized_aes = vec![
        create_test_ae_config(),
        AuthorizedAEConfig {
            ae_title: "OTHER_AE".to_string(),
            description: None,
            allowed_ips: None,
        }
    ];
    
    let provider = AuthorizedQueryProvider::new(store_handler, authorized_aes);
    
    // Test setting an authorized AE
    let authorized = provider.set_calling_ae("TEST_AE".to_string()).await;
    assert!(authorized, "TEST_AE should be authorized");
    
    // Test setting an unauthorized AE
    let unauthorized = provider.set_calling_ae("UNAUTHORIZED_AE".to_string()).await;
    assert!(!unauthorized, "UNAUTHORIZED_AE should not be authorized");
    
    // Test another authorized AE
    let other_authorized = provider.set_calling_ae("OTHER_AE".to_string()).await;
    assert!(other_authorized, "OTHER_AE should be authorized");
}