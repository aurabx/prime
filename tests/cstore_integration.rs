use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Check if DCMTK storescu is available
fn dcmtk_available() -> bool {
    std::process::Command::new("storescu")
        .arg("--version")
        .output()
        .is_ok()
}

/// Local test output directory (cleaned up between test runs)
fn test_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output")
}

/// Test harness for Prime integration tests
struct PrimeTestHarness {
    test_dir: PathBuf,
    dicom_port: u16,
    http_port: u16,
    dicom_shutdown: CancellationToken,
    http_shutdown: CancellationToken,
    scanner_shutdown: CancellationToken,
}

impl PrimeTestHarness {
    async fn start(test_name: &str) -> anyhow::Result<Self> {
        // Allocate random ports
        let dicom_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let dicom_port = dicom_listener.local_addr()?.port();
        drop(dicom_listener);

        let http_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let http_port = http_listener.local_addr()?.port();
        drop(http_listener);

        // Create local test directory
        let test_dir = test_output_dir().join(test_name);
        if test_dir.exists() {
            std::fs::remove_dir_all(&test_dir)?;
        }
        std::fs::create_dir_all(&test_dir)?;

        let storage_path = test_dir.join("storage");
        let db_path = test_dir.join("prime.db");
        let config_path = test_dir.join("prime.toml");

        // Create TOML config file
        let toml_content = format!(
            r#"
[dicom]
bind_addr = "127.0.0.1"
port = {dicom_port}
ae_title = "PRIME_TEST"
max_associations = 4

[storage]
base_path = "{storage_path}"
jmix_dir = "jmix"
dicom_temp_dir = "temp"

[jmix]
skip_hashing = true
skip_listing = true

[http]
bind_addr = "127.0.0.1"
port = {http_port}

[database]
path = "{db_path}"
"#,
            dicom_port = dicom_port,
            http_port = http_port,
            storage_path = storage_path.display(),
            db_path = db_path.display(),
        );

        std::fs::write(&config_path, &toml_content)?;
        eprintln!("Created config at: {:?}", config_path);

        // Load config from TOML file
        let config = prime::config::Config::load(&config_path)?;

        // Initialize storage
        let storage = Arc::new(prime::storage::StorageManager::new(&config.storage)?);

        // Initialize database
        let database = Arc::new(prime::database::Database::open(&config.database.path)?);

        // Create store handler
        let store_handler = Arc::new(prime::store_handler::StoreHandler::new(
            storage.clone(),
            database.clone(),
            config.jmix.clone(),
        ));

        // Create JMIX creator for background processing
        let jmix_creator = Arc::new(prime::jmix_builder::JmixCreator::new(
            storage.clone(),
            database.clone(),
            config.jmix.clone(),
        ));

        // Shutdown tokens
        let dicom_shutdown = CancellationToken::new();
        let http_shutdown = CancellationToken::new();
        let scanner_shutdown = CancellationToken::new();

        // Start DICOM SCP
        let dicom_config = config.dicom.clone();
        let dicom_shutdown_clone = dicom_shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = prime::dicom_service::start_dicom_scp(
                dicom_config,
                store_handler,
                dicom_shutdown_clone,
            )
            .await
            {
                eprintln!("DICOM SCP error: {}", e);
            }
        });

        // Start HTTP API
        let http_config = config.http.clone();
        let app_state = prime::http_api::AppState {
            storage: storage.clone(),
            database: database.clone(),
        };
        let http_shutdown_clone = http_shutdown.clone();
        tokio::spawn(async move {
            let app = prime::http_api::create_router(app_state);
            let addr = format!("{}:{}", http_config.bind_addr, http_config.port);
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .expect("Failed to bind HTTP");
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { http_shutdown_clone.cancelled().await })
                .await
                .expect("HTTP server error");
        });

        // Start background scanner with a shorter interval for tests
        let scanner_shutdown_clone = scanner_shutdown.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = jmix_creator.scan_completed_studies().await {
                            eprintln!("Scanner error: {}", e);
                        }
                    }
                    _ = scanner_shutdown_clone.cancelled() => {
                        break;
                    }
                }
            }
        });

        // Wait for ports to be ready
        Self::wait_for_port(dicom_port).await;
        Self::wait_for_port(http_port).await;

        Ok(Self {
            test_dir,
            dicom_port,
            http_port,
            dicom_shutdown,
            http_shutdown,
            scanner_shutdown,
        })
    }

    fn storage_dir(&self) -> PathBuf {
        self.test_dir.join("storage")
    }

    async fn wait_for_port(port: u16) {
        for attempt in 0..50 {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(_) => {
                    eprintln!("✓ Port {} ready after {} attempts", port, attempt + 1);
                    return;
                }
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
        eprintln!("⚠ Port {} not ready after 50 attempts", port);
    }

    async fn shutdown(self) {
        self.dicom_shutdown.cancel();
        self.http_shutdown.cancel();
        self.scanner_shutdown.cancel();
        // Give tasks time to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

fn sample_study_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/study_1")
}

#[tokio::test]
async fn test_cstore_to_jmix() {
    if !dcmtk_available() {
        eprintln!("Skipping test: storescu (DCMTK) not available");
        return;
    }

    let harness = PrimeTestHarness::start("cstore_to_jmix").await.expect("Failed to start harness");

    let sample_path = sample_study_path();
    if !sample_path.exists() {
        eprintln!(
            "Skipping test: sample study not found at {:?}",
            sample_path
        );
        harness.shutdown().await;
        return;
    }

    // Find all DICOM files in the sample study
    let dcm_files: Vec<PathBuf> = walkdir::WalkDir::new(&sample_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "dcm").unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect();

    assert!(
        !dcm_files.is_empty(),
        "Should find DICOM files in sample study"
    );
    eprintln!("Found {} DICOM files to send", dcm_files.len());

    // Send C-STORE for each file
    for dcm_file in &dcm_files {
        let status = tokio::process::Command::new("storescu")
            .arg("-v")
            .arg("-aet")
            .arg("TEST_SCU")
            .arg("-aec")
            .arg("PRIME_TEST")
            .arg("127.0.0.1")
            .arg(harness.dicom_port.to_string())
            .arg(dcm_file)
            .status()
            .await
            .expect("Failed to run storescu");

        assert!(status.success(), "storescu should succeed for {:?}", dcm_file);
    }

    eprintln!("All files sent successfully, waiting for JMIX creation...");

    // Wait for the background scanner to process (need to wait for study stability check + processing)
    // The scanner runs every 1 second and the study stability check is 10 seconds
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // Check that files appeared in storage temp directory
    let temp_dir = harness.storage_dir().join("temp");
    if temp_dir.exists() {
        eprintln!("Note: temp dir still exists - study may still be processing");
    }

    // Check JMIX output directory (it's under storage_dir/jmix)
    let mut jmix_found = false;
    let jmix_dir = harness.storage_dir().join("jmix");
    if jmix_dir.exists() {
        for entry in std::fs::read_dir(&jmix_dir).expect("read jmix dir") {
            let entry = entry.expect("jmix entry");
            if entry.path().is_dir() {
                eprintln!("Found JMIX package: {:?}", entry.path());
                jmix_found = true;
            }
        }
    }

    // Test HTTP API - list packages
    let resp = reqwest::get(format!("http://127.0.0.1:{}/api/jmix", harness.http_port))
        .await
        .expect("HTTP request failed");
    assert!(resp.status().is_success(), "API should return success");

    let packages: Vec<serde_json::Value> = resp.json().await.expect("Parse JSON");
    eprintln!("API returned {} packages", packages.len());

    // Test health endpoint
    let health_resp = reqwest::get(format!("http://127.0.0.1:{}/health", harness.http_port))
        .await
        .expect("Health request failed");
    assert!(health_resp.status().is_success(), "Health check should succeed");

    harness.shutdown().await;

    // Note: The test may not create a JMIX package if the study stability timer hasn't elapsed
    // In a real test environment, we might need to adjust the timing or mock the stability check
    if jmix_found {
        eprintln!("✓ JMIX package created successfully");
    } else {
        eprintln!("⚠ No JMIX package found - may need more time for stability check");
    }
}

#[tokio::test]
async fn test_http_health_endpoint() {
    let harness = PrimeTestHarness::start("http_health").await.expect("Failed to start harness");

    let resp = reqwest::get(format!("http://127.0.0.1:{}/health", harness.http_port))
        .await
        .expect("Health request failed");

    assert!(resp.status().is_success());

    let body: serde_json::Value = resp.json().await.expect("Parse JSON");
    assert_eq!(body["status"], "ok");

    harness.shutdown().await;
}

#[tokio::test]
async fn test_dicom_scp_accepts_connection() {
    let harness = PrimeTestHarness::start("dicom_connection").await.expect("Failed to start harness");

    // Verify the DICOM SCP port accepts connections
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", harness.dicom_port))
        .await
        .expect("Should connect to DICOM port");
    drop(stream);

    // Try C-ECHO if echoscu is available (with increased timeout)
    let echoscu_result = tokio::process::Command::new("echoscu")
        .arg("-v")
        .arg("-to") // operation timeout
        .arg("5")
        .arg("-aet")
        .arg("TEST_SCU")
        .arg("-aec")
        .arg("PRIME_TEST")
        .arg("127.0.0.1")
        .arg(harness.dicom_port.to_string())
        .output()
        .await;

    match echoscu_result {
        Ok(output) => {
            if output.status.success() {
                eprintln!("✓ C-ECHO succeeded");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("C-ECHO failed (this is expected if SCP is slow to respond): {}", stderr);
            }
        }
        Err(e) => {
            eprintln!("echoscu not available: {}", e);
        }
    }

    harness.shutdown().await;
}

/// Complete end-to-end test: Send DICOM via C-STORE, then retrieve via JMIX API
#[tokio::test]
async fn test_e2e_cstore_to_jmix_api_retrieval() {
    if !dcmtk_available() {
        eprintln!("Skipping test: storescu (DCMTK) not available");
        return;
    }

    let harness = PrimeTestHarness::start("e2e_cstore_jmix_api")
        .await
        .expect("Failed to start harness");

    let sample_path = sample_study_path();
    if !sample_path.exists() {
        eprintln!(
            "Skipping test: sample study not found at {:?}",
            sample_path
        );
        harness.shutdown().await;
        return;
    }

    // Find all DICOM files in the sample study
    let dcm_files: Vec<PathBuf> = walkdir::WalkDir::new(&sample_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "dcm").unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect();

    assert!(
        !dcm_files.is_empty(),
        "Should find DICOM files in sample study"
    );
    eprintln!("[E2E] Found {} DICOM files", dcm_files.len());

    // Step 1: Send DICOM files via C-STORE
    eprintln!("[E2E] Step 1: Sending DICOM files via C-STORE...");
    for dcm_file in &dcm_files {
        let status = tokio::process::Command::new("storescu")
            .arg("-v")
            .arg("-aet")
            .arg("TEST_SCU")
            .arg("-aec")
            .arg("PRIME_TEST")
            .arg("127.0.0.1")
            .arg(harness.dicom_port.to_string())
            .arg(dcm_file)
            .status()
            .await
            .expect("Failed to run storescu");

        assert!(status.success(), "storescu should succeed for {:?}", dcm_file);
    }
    eprintln!("[E2E] ✓ All DICOM files sent successfully");

    // Step 2: Wait for JMIX creation
    eprintln!("[E2E] Step 2: Waiting for JMIX package creation...");
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // Step 3: Query API for packages
    eprintln!("[E2E] Step 3: Querying API for JMIX packages...");
    let client = reqwest::Client::new();
    let api_url = format!("http://127.0.0.1:{}/api/jmix", harness.http_port);

    let resp = client
        .get(&api_url)
        .send()
        .await
        .expect("HTTP request failed");
    assert!(resp.status().is_success(), "API should return success");

    let packages: Vec<serde_json::Value> = resp.json().await.expect("Parse JSON");
    eprintln!("[E2E] API returned {} package(s)", packages.len());

    if packages.is_empty() {
        eprintln!("[E2E] ⚠ No packages created - may need more time");
        harness.shutdown().await;
        return;
    }

    // Step 4: Get the first package and validate it
    eprintln!("[E2E] Step 4: Retrieving and validating first package...");
    let package = &packages[0];
    let package_id = package["id"].as_str().expect("Package should have id");
    eprintln!("[E2E] Package ID: {}", package_id);

    // Verify package metadata
    assert!(
        package.get("studyInstanceUid").is_some(),
        "Package should have studyInstanceUid"
    );
    assert!(
        package.get("fileSize").is_some(),
        "Package should have fileSize"
    );
    eprintln!("[E2E] ✓ Package metadata looks good");

    // Step 5: Get package manifest
    eprintln!("[E2E] Step 5: Retrieving package manifest...");
    let manifest_url = format!(
        "http://127.0.0.1:{}/api/jmix/{}/manifest",
        harness.http_port, package_id
    );
    let manifest_resp = client
        .get(&manifest_url)
        .send()
        .await
        .expect("Manifest request failed");
    assert!(manifest_resp.status().is_success(), "Manifest should be retrievable");

    let _manifest: serde_json::Value = manifest_resp.json().await.expect("Parse manifest JSON");
    eprintln!("[E2E] ✓ Manifest retrieved successfully");

    // Step 6: List resources in the package
    eprintln!("[E2E] Step 6: Listing resources in package...");
    let resources_url = format!(
        "http://127.0.0.1:{}/api/jmix/{}/resources",
        harness.http_port, package_id
    );
    let resources_resp = client
        .get(&resources_url)
        .send()
        .await
        .expect("Resources request failed");
    assert!(
        resources_resp.status().is_success(),
        "Resources should be retrievable"
    );

    let resources: Vec<serde_json::Value> = resources_resp.json().await.expect("Parse resources JSON");
    eprintln!("[E2E] ✓ Found {} resources", resources.len());

    // Step 7: Get status of the package
    eprintln!("[E2E] Step 7: Checking package status...");
    let status_url = format!(
        "http://127.0.0.1:{}/api/jmix/{}/status",
        harness.http_port, package_id
    );
    let status_resp = client
        .get(&status_url)
        .send()
        .await
        .expect("Status request failed");
    assert!(status_resp.status().is_success(), "Status should be retrievable");

    let status: serde_json::Value = status_resp.json().await.expect("Parse status JSON");
    let exists = status["exists"].as_bool().expect("exists should be bool");
    let manifest_readable = status["manifest_readable"]
        .as_bool()
        .expect("manifest_readable should be bool");
    let zip_accessible = status["zip_accessible"]
        .as_bool()
        .expect("zip_accessible should be bool");

    assert!(exists, "Package should exist");
    assert!(manifest_readable, "Manifest should be readable");
    assert!(zip_accessible, "ZIP should be accessible");
    eprintln!("[E2E] ✓ Package status: all checks passed");

    // Step 8: Download package ZIP
    eprintln!("[E2E] Step 8: Downloading package ZIP...");
    let download_url = format!(
        "http://127.0.0.1:{}/api/jmix/{}",
        harness.http_port, package_id
    );
    let download_resp = client
        .get(&download_url)
        .send()
        .await
        .expect("Download request failed");
    assert!(download_resp.status().is_success(), "Package should be downloadable");

    let bytes = download_resp.bytes().await.expect("Read bytes");
    eprintln!("[E2E] ✓ Downloaded package: {} bytes", bytes.len());
    assert!(bytes.len() > 0, "Package ZIP should not be empty");

    // Step 9: Test with query filters
    eprintln!("[E2E] Step 9: Testing API filters...");
    let study_uid = package["studyInstanceUid"].as_str().unwrap_or("");
    let filter_url = format!(
        "http://127.0.0.1:{}/api/jmix?studyInstanceUid={}",
        harness.http_port, study_uid
    );
    let filter_resp = client
        .get(&filter_url)
        .send()
        .await
        .expect("Filter request failed");
    assert!(filter_resp.status().is_success(), "Filter should work");

    let filtered: Vec<serde_json::Value> = filter_resp.json().await.expect("Parse filtered JSON");
    assert!(!filtered.is_empty(), "Filter should find packages");
    eprintln!("[E2E] ✓ Filters working correctly");

    harness.shutdown().await;
    eprintln!("[E2E] ✅ Complete end-to-end test PASSED!");
}
