//! Canvas - image processing server.
//!
//! It uses libvips for photo processing and redis for cache.
//!
//! HTTP API is powered by Axum.
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router, Server,
};
use libvips::VipsApp;
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use std::fs;

// Re-exports
pub use config::Config;
pub use error::HttpError;
pub use state::AppState;

// Modules
mod api;
mod config;
mod error;
mod state;

#[tokio::main]
async fn main() {
    // Initialize libvips.
    let libvipsapp = VipsApp::new("Test Libvips", false).unwrap();
    let cpu_num: i32 = num_cpus::get().try_into().unwrap();
    println!("Starting {cpu_num} workers");
    libvipsapp.concurrency_set(cpu_num);

    // Read configuration.
    let cfg = Config::new();
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
    let axumapp = Router::new()
        .route("/health", get(api::health::get_health))
        .route("/images", post(api::upload::upload_image))
        .route("/images/:hash", get(api::image::get_image))
        .layer(DefaultBodyLimit::max(1024 * cfg.file_size_limit_kb))
        .with_state(state);

    Server::bind(&format!("0.0.0.0:{}", cfg.port).parse().unwrap())
        .serve(axumapp.into_make_service())
        .await
        .unwrap();
}
