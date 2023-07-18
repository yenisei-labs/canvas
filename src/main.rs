//! Canvas - image processing server.
//!
//! It uses libvips for photo processing and redis for cache.
//!
//! HTTP API is powered by Axum.
use axum::{
    extract::DefaultBodyLimit,
    http::Method,
    routing::{get, post},
    Router, Server,
};
use libvips::VipsApp;
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use std::fs;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use hyper::http::HeaderValue;
use log::{info, warn};

// Re-exports
pub use app_config::AppConfig;
pub use error::HttpError;
pub use state::AppState;

// Modules
mod api;
mod app_config;
mod error;
mod state;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Initialize libvips.
    let libvipsapp = VipsApp::new("Test Libvips", false).unwrap();
    let cpu_num: i32 = num_cpus::get().try_into().unwrap();
    info!("Starting {cpu_num} workers");
    libvipsapp.concurrency_set(cpu_num);

    // Read configuration.
    let cfg = app_config::get_config().unwrap();
    fs::create_dir_all(cfg.upload_dir.clone()).unwrap();

    // Connect to redis.
    let redis_client = mobc_redis::redis::Client::open(cfg.redis_url.clone()).unwrap();
    let redis_manager = RedisConnectionManager::new(redis_client);
    let redis_pool = Pool::builder()
        .max_open(cpu_num.try_into().unwrap())
        .build(redis_manager);

    // Create shared state.
    let state = AppState::new(cfg.clone(), redis_pool);

    // Initialize axum.

    // Configure CORS layer.
    let mut cors = CorsLayer::new()
        // allow `GET` and `POST` when accessing the resource
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .max_age(Duration::from_secs(60) * 10);

    match cfg.allowed_origins {
        Some(raw_list) => {
            let mut origins: Vec<HeaderValue> = Vec::new();
            for origin in raw_list.iter() {
                origins.push(origin.parse().unwrap());
            }
            cors = cors.allow_origin(origins);
        },
        None => {
            warn!("CORS: all origins are allowed");
            cors = cors.allow_origin(Any);
        },
    };

    let mut axumapp = Router::new()
        .route("/health", get(api::health::get_health))
        .route("/images", post(api::upload::upload_image))
        .route("/images/:hash", get(api::image::get_image))
        .layer(DefaultBodyLimit::max(1024 * cfg.file_size_limit_kb))
        .layer(cors)
        .with_state(state);

    if cfg.enable_tracing {
        axumapp = axumapp.layer(TraceLayer::new_for_http());
    }

    Server::bind(&format!("0.0.0.0:{}", cfg.port).parse().unwrap())
        .serve(axumapp.into_make_service())
        .await
        .unwrap();
}
