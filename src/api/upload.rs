use crate::{AppState, HttpError};
use axum::{
    body::Bytes,
    extract::{Multipart, State},
    response::{IntoResponse, Json},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Write, sync::Arc};

#[derive(Serialize)]
pub struct Response {
    pub hash: String,
}

/// Save uploaded image.
/// Url: /upload
/// Method: POST
/// Payload: image - multipart
pub async fn upload_image(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Get the first field
    let field = match multipart.next_field().await {
        Ok(field) => match field {
            Some(field) => field,
            None => return Err(HttpError::bad_request("Missing 'image' field")),
        },
        Err(err) => return Err(HttpError::bad_request(&err.to_string())),
    };

    // Get the name of the first field
    let name = match field.name() {
        Some(name) => name.to_string(),
        None => return Err(HttpError::bad_request("Missing field name")),
    };

    // Check the name
    if name != "image" {
        return Err(HttpError::bad_request(&format!(
            "Unexpected field {} (expected 'image')",
            name
        )));
    }

    // Get field data
    let data = match field.bytes().await {
        Ok(data) => data,
        Err(err) => return Err(HttpError::bad_request(&err.to_string())),
    };

    // Calculate file path
    let hash = get_file_hash(&data);
    let filepath = state.get_file_path(&hash);

    // Save file
    if !filepath.exists() {
        let mut f = match File::create(filepath) {
            Ok(f) => f,
            Err(err) => return Err(HttpError::internal_server_error(&err.to_string())),
        };

        if let Err(err) = f.write_all(&data) {
            return Err(HttpError::internal_server_error(&err.to_string()));
        }
    }

    // Return file hash
    Ok(Json(Response { hash }))
}

fn get_file_hash(data: &Bytes) -> String {
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = format!("{:x}", hasher.finalize());
    return hash;
}
