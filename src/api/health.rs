use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Response {
    pub ok: bool,
}

pub async fn get_health() -> Json<Response> {
    Json(Response { ok: true })
}
