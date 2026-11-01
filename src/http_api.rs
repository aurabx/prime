use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, debug};
use crate::storage::StorageManager;
use crate::database::{Database, JmixPackageRecord};
use tokio_util::io::ReaderStream;
use axum::body::Body;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<StorageManager>,
    pub database: Arc<Database>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "studyInstanceUid")]
    study_instance_uid: Option<String>,
    #[serde(rename = "patientId")]
    patient_id: Option<String>,
    #[serde(rename = "patientName")]
    patient_name: Option<String>,
    #[serde(rename = "startDate")]
    start_date: Option<String>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    modality: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
pub struct PackageInfo {
    id: String,
    #[serde(rename = "studyInstanceUid")]
    study_uid: String,
    #[serde(rename = "patientId")]
    patient_id: Option<String>,
    #[serde(rename = "patientName")]
    patient_name: Option<String>,
    #[serde(rename = "studyDate")]
    study_date: Option<String>,
    #[serde(rename = "studyTime")]
    study_time: Option<String>,
    modality: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: i64,
    #[serde(rename = "fileSize")]
    file_size: u64,
}

impl From<JmixPackageRecord> for PackageInfo {
    fn from(r: JmixPackageRecord) -> Self {
        Self {
            id: r.id,
            study_uid: r.study_uid,
            patient_id: r.patient_id,
            patient_name: r.patient_name,
            study_date: r.study_date,
            study_time: r.study_time,
            modality: r.modality,
            created_at: r.created_at,
            file_size: r.file_size,
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/jmix", get(list_packages_handler))
        .route("/api/jmix/{id}", get(get_package_handler)
                                     .head(head_package_handler)
                                     .delete(delete_package_handler))
        .route("/api/jmix/{id}/manifest", get(get_manifest_handler))
        .route("/api/jmix/{id}/status", get(get_package_status_handler))
        .route("/api/jmix/{id}/resources", get(list_resources_handler))
        .route("/api/jmix/{id}/resources/{rid}", get(get_resource_handler))
        .route("/api/jmix/{id}/resources/{rid}/data", get(get_resource_data_handler))
        .with_state(state)
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn list_packages_handler(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);

    debug!("GET /api/jmix with filters");

    // TODO: for now fall back to study_uid or all packages; we’ll add exact filter methods to Database later if desired
    let packages = if let Some(ref study_uid) = query.study_instance_uid {
        info!("Listing packages for study: {}", study_uid);
        state.database.get_packages_by_study(study_uid)?
    } else {
        info!("Listing all packages (filters will be applied post-query)");
        state.database.list_packages(limit + offset, 0)?
    };

    // Apply simple post-filtering for now; later replace with indexed queries for performance
    let filtered: Vec<_> = packages.into_iter().filter(|pkg| {
        if let Some(ref patient_id) = query.patient_id {
            if pkg.patient_id.as_ref() != Some(patient_id) { return false; }
        }
        if let Some(ref patient_name) = query.patient_name {
            if let Some(ref name) = pkg.patient_name {
                let name_lower = name.to_lowercase();
                let filter_lower = patient_name.to_lowercase();
                if !name_lower.contains(&filter_lower) { return false; }
            } else { return false; }
        }
        if let Some(ref modality) = query.modality {
            if pkg.modality.as_ref() != Some(modality) { return false; }
        }
        if let (Some(ref start_date), Some(ref end_date)) = (&query.start_date, &query.end_date) {
            if let Some(ref d) = pkg.study_date {
                if d < start_date || d > end_date { return false; }
            } else { return false; }
        }
        true
    }).collect();

    let paginated = filtered.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    debug!("Returning {} packages (post-filter)", paginated.len());
    let infos: Vec<PackageInfo> = paginated.into_iter().map(Into::into).collect();
    Ok(Json(infos))
}

async fn get_package_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    info!("GET /api/jmix/{} - downloading package", id);
    let _pkg = state.database.get_package(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let zip_path = state.storage.jmix_package_zip(&id);
    if !zip_path.exists() {
        return Err(AppError::NotFound(format!("Package ZIP not found: {}", id)));
    }

    let file = tokio::fs::File::open(&zip_path).await
        .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/zip")
        .header("Content-Disposition", format!("attachment; filename=\"{}.zip\"", id))
        .body(body)
        .unwrap())
}

async fn head_package_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let _pkg = state.database.get_package(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let zip_path = state.storage.jmix_package_zip(&id);
    if !zip_path.exists() {
        return Err(AppError::NotFound(format!("Package ZIP not found: {}", id)));
    }

    let metadata = tokio::fs::metadata(&zip_path).await
        .map_err(|e| AppError::Internal(format!("Failed to read metadata: {}", e)))?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/zip")
        .header("Content-Length", metadata.len().to_string())
        .body(Body::empty())
        .unwrap())
}

async fn get_manifest_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /api/jmix/{}/manifest", id);
    let json = read_manifest(&state, &id).await?;
    Ok(Json(json))
}

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

// Helper: read manifest from disk or ZIP
async fn read_manifest(state: &AppState, id: &str) -> Result<serde_json::Value, AppError> {
    let _pkg = state.database.get_package(id)?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let pkg_dir = state.storage.jmix_package_dir(id);
    let manifest_path = pkg_dir.join("manifest.json");

    if manifest_path.exists() {
        let content = tokio::fs::read_to_string(&manifest_path).await
            .map_err(|e| AppError::Internal(format!("Failed to read manifest: {}", e)))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| AppError::Internal(format!("Failed to parse manifest: {}", e)))?;
        return Ok(json);
    }

    let zip_path = state.storage.jmix_package_zip(id);
    if zip_path.exists() {
        let zip_bytes = tokio::fs::read(&zip_path).await
            .map_err(|e| AppError::Internal(format!("Failed to read ZIP: {}", e)))?;
        if let Some(json) = manifest_from_zip(zip_bytes)? {
            return Ok(json);
        }
    }

    Err(AppError::NotFound("Manifest not found".to_string()))
}

/// Reads the first `manifest.json` from a stored package ZIP.
/// A corrupt or hostile archive is an error, so it never looks like a missing manifest.
fn manifest_from_zip(zip_bytes: Vec<u8>) -> Result<Option<serde_json::Value>, AppError> {
    use std::io::Read;

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| AppError::Internal(format!("Failed to open ZIP: {}", e)))?;
    archive
        .validate_header_consistency()
        .map_err(|e| AppError::Internal(format!("ZIP headers are inconsistent: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(format!("Failed to read ZIP entry {}: {}", i, e)))?;
        let name = file
            .name()
            .map_err(|e| AppError::Internal(format!("Failed to read ZIP entry {} name: {}", i, e)))?;
        if name.ends_with("manifest.json") {
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| AppError::Internal(format!("Failed to read manifest from ZIP: {}", e)))?;
            let json: serde_json::Value = serde_json::from_str(&contents)
                .map_err(|e| AppError::Internal(format!("Failed to parse manifest: {}", e)))?;
            return Ok(Some(json));
        }
    }

    Ok(None)
}

#[derive(Serialize)]
struct ResourceSummary {
    id: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    resource_type: Option<String>,
    path: Option<String>,
    size: Option<u64>,
}

async fn list_resources_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /api/jmix/{}/resources", id);
    let manifest = read_manifest(&state, &id).await?;
    let mut out: Vec<ResourceSummary> = Vec::new();
    if let Some(resources) = manifest.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            let rid = r.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let rtype = r.get("type").and_then(|v| v.as_str()).map(str::to_string);
            let path = r.get("path").and_then(|v| v.as_str()).map(str::to_string);
            let size = r.get("size").and_then(|v| v.as_u64());
            if !rid.is_empty() {
                out.push(ResourceSummary { id: rid, resource_type: rtype, path, size });
            }
        }
    }
    Ok(Json(out))
}

async fn get_resource_handler(
    State(state): State<AppState>,
    Path((id, rid)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /api/jmix/{}/resources/{}", id, rid);
    let manifest = read_manifest(&state, &id).await?;
    if let Some(resources) = manifest.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            if r.get("id").and_then(|v| v.as_str()) == Some(rid.as_str()) {
                return Ok(Json(r.clone()));
            }
        }
    }
    Err(AppError::NotFound(format!("Resource not found: {}", rid)))
}

fn resolve_resource_path(pkg_dir: &std::path::Path, rid: &str, resource: &serde_json::Value) -> std::path::PathBuf {
    if let Some(p) = resource.get("path").and_then(|v| v.as_str()) {
        let candidate = pkg_dir.join(p);
        if candidate.exists() { return candidate; }
    }
    // common fallbacks
    let by_id = pkg_dir.join("resources").join(rid);
    if by_id.exists() { return by_id; }
    let by_id_json = pkg_dir.join(format!("{}.json", rid));
    if by_id_json.exists() { return by_id_json; }
    pkg_dir.join(rid) // last resort
}

async fn get_resource_data_handler(
    State(state): State<AppState>,
    Path((id, rid)): Path<(String, String)>,
) -> Result<Response, AppError> {
    info!("GET /api/jmix/{}/resources/{}/data", id, rid);
    let manifest = read_manifest(&state, &id).await?;
    let pkg_dir = state.storage.jmix_package_dir(&id);

    if let Some(resources) = manifest.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            if r.get("id").and_then(|v| v.as_str()) == Some(rid.as_str()) {
                let path = resolve_resource_path(&pkg_dir, &rid, r);
                if !path.exists() {
                    return Err(AppError::NotFound(format!("Resource file not found: {}", rid)));
                }
                let file = tokio::fs::File::open(&path).await
                    .map_err(|e| AppError::Internal(format!("Failed to open resource: {}", e)))?;
                let stream = ReaderStream::new(file);
                let body = Body::from_stream(stream);
                let content_type = r.get("contentType").and_then(|v| v.as_str()).unwrap_or("application/octet-stream");
                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", content_type)
                    .header("Content-Disposition", format!("inline; filename=\"{}\"", path.file_name().and_then(|s| s.to_str()).unwrap_or(&rid)))
                    .body(body)
                    .unwrap());
            }
        }
    }

    Err(AppError::NotFound(format!("Resource not found: {}", rid)))
}

async fn delete_package_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!("DELETE /api/jmix/{}", id);
    
    // Verify package exists
    let _pkg = state.database.get_package(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let pkg_dir = state.storage.jmix_package_dir(&id);
    let _zip_path = state.storage.jmix_package_zip(&id);

    // Delete all files
    if pkg_dir.exists() {
        tokio::fs::remove_dir_all(&pkg_dir).await
            .map_err(|e| AppError::Internal(format!("Failed to delete package directory: {}", e)))?;
    }

    // Remove from database indexes
    state.database.delete_package(&id)?;

    let response_data = serde_json::json!({
        "success": true,
        "message": format!("Package {} deleted successfully", id)
    });
    
    Ok(Json(response_data))
}

#[derive(Serialize)]
struct PackageStatus {
    id: String,
    exists: bool,
    manifest_readable: bool,
    zip_accessible: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
}

async fn get_package_status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /api/jmix/{}/status", id);

    let pkg = state.database.get_package(&id)?;
    
    // If package doesn't exist, return 404
    let _pkg = pkg.ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let manifest_readable = match read_manifest(&state, &id).await {
        Ok(_) => true,
        Err(_) => false,
    };

    let zip_path = state.storage.jmix_package_zip(&id);
    let zip_accessible = zip_path.exists();

    let mut issues = Vec::new();
    if !manifest_readable {
        issues.push("Manifest not accessible (could be corrupted or missing)".to_string());
    }
    if !zip_accessible {
        issues.push("Package ZIP file missing or inaccessible".to_string());
    }

    let status = PackageStatus {
        id: id.clone(),
        exists: true,
        manifest_readable,
        zip_accessible,
        issues,
    };

    Ok(Json(status))
}

#[cfg(test)]
mod manifest_zip_tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            for (name, data) in entries {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer
    }

    #[test]
    fn reads_manifest_from_a_valid_zip() {
        let bytes = zip_with(&[("pkg/manifest.json", br#"{"id":"x"}"#)]);
        let json = manifest_from_zip(bytes).ok().flatten().unwrap();
        assert_eq!(json["id"], "x");
    }

    #[test]
    fn returns_none_when_no_manifest_exists() {
        let bytes = zip_with(&[("pkg/other.json", b"{}")]);
        assert!(matches!(manifest_from_zip(bytes), Ok(None)));
    }

    #[test]
    fn rejects_mismatched_headers_even_with_a_valid_later_manifest() {
        let mut bytes = zip_with(&[
            ("aaa/manifest.json", br#"{"who":"first"}"#),
            ("zzz/manifest.json", br#"{"who":"second"}"#),
        ]);
        // The first match is the local header, so the central directory keeps the old name.
        let at = bytes
            .windows(3)
            .position(|w| w == b"aaa")
            .unwrap();
        bytes[at..at + 3].copy_from_slice(b"bbb");
        assert!(matches!(
            manifest_from_zip(bytes),
            Err(AppError::Internal(_))
        ));
    }
}
