//! Smoke tests for JMIX API endpoints
//! These tests verify basic functionality of all public API endpoints.

use reqwest::blocking::Client;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

static PORT: AtomicU16 = AtomicU16::new(49000);

fn spawn_test_server() -> (String, tempfile::TempDir, Arc<Runtime>) {
    let p = PORT.fetch_add(1, Ordering::SeqCst);
    let port = p % 1000 + 40000;
    let tmp = tempfile::TempDir::new().unwrap();
    let storage_cfg = prime::config::StorageConfig {
        base_path: tmp.path().to_path_buf(),
        jmix_dir: "jmix-packages".to_string(),
        dicom_temp_dir: "dicom-temp".to_string(),
    };
    let storage = Arc::new(prime::storage::StorageManager::new(&storage_cfg).unwrap());
    let db_path = tmp.path().join("prime.redb");
    let database = Arc::new(prime::database::Database::open(&db_path).unwrap());

    let state = prime::http_api::AppState { storage: storage.clone(), database: database.clone() };
    let app = prime::http_api::create_router(state);

    let rt = Arc::new(Runtime::new().unwrap());

    rt.block_on(async move {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
    });

    std::thread::sleep(Duration::from_millis(50));

    (format!("http://127.0.0.1:{}", port), tmp, rt)
}

#[test]
fn smoke_health_endpoint() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();
    let res = client.get(format!("{}/health", base)).send().unwrap();
    assert!(res.status().is_success());
    assert!(res.text().unwrap().contains("ok"));
}

#[test]
fn smoke_list_packages_empty() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();
    let res = client.get(format!("{}/api/jmix", base)).send().unwrap();
    assert!(res.status().is_success());
    let body: serde_json::Value = res.json().unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[test]
fn smoke_list_packages_with_filters() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    // Test with various query parameters (should not error even with empty results)
    let queries = vec![
        "patientId=123",
        "patientName=John",
        "modality=CT",
        "startDate=2024-01-01&endDate=2024-01-31",
        "limit=10&offset=0",
    ];

    for q in queries {
        let res = client
            .get(format!("{}/api/jmix?{}", base, q))
            .send()
            .unwrap();
        assert!(res.status().is_success(), "Failed for query: {}", q);
        let body: serde_json::Value = res.json().unwrap();
        assert!(body.is_array(), "Expected array for query: {}", q);
    }
}

#[test]
fn smoke_get_package_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_head_package_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .head(format!("{}/api/jmix/nonexistent", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_manifest_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent/manifest", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_list_resources_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent/resources", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_get_resource_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent/resources/rid123", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_get_resource_data_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent/resources/rid123/data", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_status_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent/status", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_delete_not_found() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .delete(format!("{}/api/jmix/nonexistent", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn smoke_all_endpoints_reachable() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    // Test that all endpoints are reachable (though may return 404)
    let endpoints = vec![
        ("/health", "GET"),
        ("/api/jmix", "GET"),
        ("/api/jmix/test-id", "GET"),
        ("/api/jmix/test-id", "HEAD"),
        ("/api/jmix/test-id", "DELETE"),
        ("/api/jmix/test-id/manifest", "GET"),
        ("/api/jmix/test-id/status", "GET"),
        ("/api/jmix/test-id/resources", "GET"),
        ("/api/jmix/test-id/resources/rid", "GET"),
        ("/api/jmix/test-id/resources/rid/data", "GET"),
    ];

    for (path, method) in endpoints {
        let url = format!("{}{}", base, path);
        let res = match method {
            "GET" => client.get(&url).send().unwrap(),
            "HEAD" => client.head(&url).send().unwrap(),
            "DELETE" => client.delete(&url).send().unwrap(),
            _ => panic!("Unknown method: {}", method),
        };
        // Just check it doesn't 500
        assert!(
            res.status().is_client_error() || res.status().is_success(),
            "Endpoint {} {} returned: {}",
            method,
            path,
            res.status()
        );
    }
}

#[test]
fn smoke_error_responses_are_json() {
    let (base, _tmp, _rt) = spawn_test_server();
    let client = Client::new();

    let res = client
        .get(format!("{}/api/jmix/nonexistent", base))
        .send()
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::NOT_FOUND);
    let body: serde_json::Value = res.json().unwrap();
    assert!(body.get("error").is_some(), "Error response should contain 'error' field");
}
