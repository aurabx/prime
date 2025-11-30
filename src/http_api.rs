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
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
pub struct PackageInfo {
    id: String,
    study_uid: String,
    created_at: i64,
    file_size: u64,
}

impl From<JmixPackageRecord> for PackageInfo {
    fn from(r: JmixPackageRecord) -> Self {
        Self {
            id: r.id,
            study_uid: r.study_uid,
            created_at: r.created_at,
            file_size: r.file_size,
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/jmix", get(list_packages_handler))
        .route("/api/jmix/{id}", get(get_package_handler).head(head_package_handler))
        .route("/api/jmix/{id}/manifest", get(get_manifest_handler))
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

    debug!("GET /api/jmix - study_uid={:?}, limit={}, offset={}", query.study_instance_uid, limit, offset);

    let packages = if let Some(ref study_uid) = query.study_instance_uid {
        info!("Listing packages for study: {}", study_uid);
        state.database.get_packages_by_study(study_uid)?
    } else {
        info!("Listing all packages (limit={}, offset={})", limit, offset);
        state.database.list_packages(limit, offset)?
    };

    debug!("Found {} packages", packages.len());
    let infos: Vec<PackageInfo> = packages.into_iter().map(Into::into).collect();
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
    let _pkg = state.database.get_package(&id)?
        .ok_or_else(|| AppError::NotFound(format!("Package not found: {}", id)))?;

    let pkg_dir = state.storage.jmix_package_dir(&id);
    let manifest_path = pkg_dir.join("manifest.json");

    if manifest_path.exists() {
        let content = tokio::fs::read_to_string(&manifest_path).await
            .map_err(|e| AppError::Internal(format!("Failed to read manifest: {}", e)))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| AppError::Internal(format!("Failed to parse manifest: {}", e)))?;
        return Ok(Json(json));
    }

    // Try to extract from ZIP
    let zip_path = state.storage.jmix_package_zip(&id);
    if zip_path.exists() {
        let zip_bytes = tokio::fs::read(&zip_path).await
            .map_err(|e| AppError::Internal(format!("Failed to read ZIP: {}", e)))?;
        
        let cursor = std::io::Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| AppError::Internal(format!("Failed to open ZIP: {}", e)))?;

        for i in 0..archive.len() {
            if let Ok(mut file) = archive.by_index(i) {
                if file.name().ends_with("manifest.json") {
                    use std::io::Read;
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)
                        .map_err(|e| AppError::Internal(format!("Failed to read manifest from ZIP: {}", e)))?;
                    let json: serde_json::Value = serde_json::from_str(&contents)
                        .map_err(|e| AppError::Internal(format!("Failed to parse manifest: {}", e)))?;
                    return Ok(Json(json));
                }
            }
        }
    }

    Err(AppError::NotFound("Manifest not found".to_string()))
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
