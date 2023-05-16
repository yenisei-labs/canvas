use std::env;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    // Directory where uploaded files will be saved (default: 'uploads')
    pub upload_dir: String,
    /// File size limit in kilobytes (default: 4096)
    pub file_size_limit_kb: usize,
    /// Server port (default: 3000)
    pub port: u16,
    /// Redis URL
    pub redis_url: String,
    /// Watermark file path (example: '/app/watermark.png')
    pub watermark_file_path: Option<String>,
}

impl Config {
    pub fn new() -> Config {
        let _ = dotenvy::dotenv();
        Config {
            upload_dir: env::var("CANVAS_UPLOAD_DIR").unwrap_or("uploads".to_string()),
            file_size_limit_kb: env::var("CANVAS_FILE_SIZE_LIMIT_KB")
                .unwrap_or("4096".to_string())
                .parse()
                .unwrap(),
            port: env::var("CANVAS_PORT")
                .unwrap_or("3000".to_string())
                .parse()
                .unwrap(),
            redis_url: env::var("CANVAS_REDIS_URL").unwrap_or("redis://127.0.0.1/".to_string()),
            watermark_file_path: match env::var("CANVAS_WATERMARK_FILE_PATH") {
                Ok(val) => Some(val),
                Err(_) => None,
            },
        }
    }
}
