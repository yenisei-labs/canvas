use config::Config;

/// Server configuration.
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    // Directory where uploaded files will be saved (default: 'uploads')
    pub upload_dir: String,
    /// File size limit in kilobytes (default: 4096)
    pub file_size_limit_kb: usize,
    /// Server port (default: 3000)
    pub port: u16,
    /// Redis URL (default: "redis://127.0.0.1/")
    pub redis_url: String,
    /// Watermark file path (example: '/app/watermark.png')
    pub watermark_file_path: Option<String>,
}

pub fn get_config() -> anyhow::Result<AppConfig> {
    let _ = dotenvy::dotenv();

    let config = Config::builder()
        .set_default("upload_dir", "uploads")?
        .set_default("file_size_limit_kb", 4096)?
        .set_default("port", 3000)?
        .set_default("redis_url", "redis://127.0.0.1/")?
        .add_source(
            config::Environment::with_prefix("CANVAS")
                .try_parsing(true)
                .separator("_"),
        )
        .build()?;

    let my_config: AppConfig = config.try_deserialize()?;

    Ok(my_config)
}
