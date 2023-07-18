use crate::app_config::AppConfig;
use libvips::VipsImage;
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Shared application state.
pub struct AppState {
    /// Server configuration.
    pub cfg: AppConfig,
    /// Redis connection pool.
    pub redis: Pool<RedisConnectionManager>,
    /// Buffer with watermark.
    /// (VipsImage cannot be passed between threads)
    pub watermark: Option<Vec<u8>>,
}

impl AppState {
    /// Create new instance of application state.
    pub fn new(cfg: AppConfig, redis: Pool<RedisConnectionManager>) -> Arc<AppState> {
        // Preload watermark
        let watermark = match &cfg.watermark_file_path {
            Some(path) => {
                let image = VipsImage::new_from_file(path).unwrap();
                Some(image.image_write_to_buffer(".png").unwrap())
            }
            None => None,
        };

        Arc::new(AppState {
            cfg,
            redis,
            watermark,
        })
    }

    /// Get path to uploaded file by hash (id).
    pub fn get_file_path(&self, hash: &str) -> PathBuf {
        Path::new(&self.cfg.upload_dir).join(hash)
    }
}
