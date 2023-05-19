use crate::{AppState, HttpError};
use axum::{
    extract::{Path, Query, State},
    http::{
        header::{self, HeaderMap},
        status::StatusCode,
    },
    response::IntoResponse,
};
use libvips::{ops, VipsImage};
use mobc_redis::redis::AsyncCommands;
use std::{cmp, collections::HashMap, fmt, path::PathBuf, sync::Arc};

#[derive(Debug)]
pub enum ImageFormat {
    Webp,
    Jpeg,
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ImageFormat::Jpeg => "jpeg",
                ImageFormat::Webp => "webp",
            }
        )
    }
}

#[derive(Debug)]
pub struct ImageProps {
    pub width: u16,
    pub height: u16,
    pub quality: u8,
    /// Add a pre-configured watermark on top of a photo?
    pub watermark: bool,
    pub format: ImageFormat,
    pub filename: Option<String>,
    /// Small text to be added to the top left corner.
    /// Can be used instead of a watermark.
    pub overlay: Option<String>,
}

impl Default for ImageProps {
    fn default() -> ImageProps {
        ImageProps {
            width: 1024,
            height: 1024,
            quality: 80,
            watermark: false,
            format: ImageFormat::Webp,
            filename: None,
            overlay: None,
        }
    }
}

impl ImageProps {
    /// Parse URL parameters.
    fn from_params(params: &HashMap<String, String>) -> ImageProps {
        let mut image_props = ImageProps::default();

        if let Some(value) = params.get("width") {
            if let Ok(width) = value.parse() {
                image_props.width = width;
            }
        }

        if let Some(value) = params.get("height") {
            if let Ok(height) = value.parse() {
                image_props.height = height;
            }
        }

        if let Some(value) = params.get("quality") {
            if let Ok(quality) = value.parse() {
                image_props.quality = quality;
            }
        }

        if let Some(_) = params.get("watermark") {
            image_props.watermark = true;
        }

        if let Some(value) = params.get("format") {
            image_props.format = match value.as_str() {
                "jpg" | "jpeg" => ImageFormat::Jpeg,
                _ => ImageFormat::Webp,
            }
        }

        if let Some(filename) = params.get("filename") {
            image_props.filename = Some(filename.to_string());
        }

        if let Some(overlay) = params.get("overlay") {
            image_props.overlay = Some(overlay.to_string());
        }

        image_props
    }
}

/// Convert image.
/// Method: GET.
/// Possible parameters: see ImageProps.
pub async fn get_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(hash): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Check if the image was uploaded to the server.
    let filepath = state.get_file_path(&hash);
    if !filepath.exists() {
        return Err(HttpError::not_found(&format!(
            "Image {} was not found",
            hash
        )));
    }

    // Check if-none-match header
    let image_props = ImageProps::from_params(&params);
    let image_id = get_image_id(&hash, &image_props);
    let response_headers = get_headers(&image_props, &image_id, &hash);
    if headers.contains_key("If-None-Match") {
        println!("Found if-none-match header: {}", image_id);
        return Ok((StatusCode::NOT_MODIFIED, response_headers, Vec::new()));
    }

    // Check redis cache.
    let mut redis_con = state.redis.get().await.unwrap();
    let exists = redis_con.exists(&image_id).await.unwrap();

    if exists {
        println!("Using cached image {}", image_id);
        let image: Vec<u8> = redis_con.get(&image_id).await.unwrap();
        return Ok((StatusCode::OK, response_headers, image));
    }

    println!("Image was not found in cache: {}", image_id);
    let buffer = match process_image(filepath, &image_props, state) {
        Ok(buffer) => buffer,
        Err(err) => return Err(HttpError::internal_server_error(&err.to_string())),
    };

    // Save to redis cache
    let _: () = redis_con.set(image_id, &buffer).await.unwrap();

    Ok((StatusCode::OK, response_headers, buffer))
}

/// Calculate unique ID for this image.
/// It takes height, width, quality, format and watermark into account.
/// Image ID will be used as a key for caching.
pub fn get_image_id(hash: &str, props: &ImageProps) -> String {
    format!(
        "{}-{}-{}-{}-{}-{}-{}",
        hash,
        props.width,
        props.height,
        props.quality,
        props.watermark,
        props.format,
        props.overlay.clone().unwrap_or("none".to_string())
    )
}

/// Rotate, crop, apply watermark and encode requested image.
/// Returns encoded image in any of the supported formats.
fn process_image(
    filepath: PathBuf,
    image_props: &ImageProps,
    state: Arc<AppState>,
) -> anyhow::Result<Vec<u8>> {
    let image = VipsImage::new_from_file(&filepath.into_os_string().into_string().unwrap())?;

    // Apply rotation from EXIF tag.
    let rotated_image = ops::autorot(&image)?;

    // Resize the image so that the smaller side of the image is fully visible
    let original_width = rotated_image.get_width();
    let original_height = rotated_image.get_height();

    let width_scale_factor: f64 = f64::from(image_props.width) / f64::from(original_width);
    let height_scale_factor: f64 = f64::from(image_props.height) / f64::from(original_height);

    let min_factor = width_scale_factor.max(height_scale_factor).min(1.0);
    let resized_image = ops::resize(&rotated_image, min_factor)?;

    // Crop big side with smart algorithm
    let cropped_image = ops::smartcrop(
        &resized_image,
        cmp::min(image_props.width.into(), resized_image.get_width()),
        cmp::min(image_props.height.into(), resized_image.get_height()),
    )?;

    // Add watermark if needed.
    let image_with_watermark = match image_props.watermark {
        true => match &state.watermark {
            Some(watermark_buffer) => {
                // I have to load this picture every time again, because it cannot be passed between threads.
                let watermark = VipsImage::new_from_buffer(&watermark_buffer, "")?;

                // Join images.
                ops::composite_2(&cropped_image, &watermark, ops::BlendMode::Screen)?
            }
            // Watermark image is undefined
            None => cropped_image,
        },
        // Watermark not required
        false => cropped_image,
    };

    // Add overlay.
    let image_with_overlay = match &image_props.overlay {
        Some(overlay) => {
            let text = ops::text(&overlay)?;
            let white = ops::copy_with_opts(
                &VipsImage::new_from_image(&text, &[170.0, 170.0, 170.0])?,
                &ops::CopyOptions {
                    interpretation: ops::Interpretation::Srgb,
                    ..ops::CopyOptions::default()
                },
            )?;
            let overlay = ops::bandjoin(&mut [white, text])?;
            ops::composite_2(&image_with_watermark, &overlay, ops::BlendMode::Screen)?
        }
        None => image_with_watermark,
    };

    // Encode image.
    match image_props.format {
        ImageFormat::Webp => {
            let options = get_webp_options(image_props.quality);
            let buffer = ops::webpsave_buffer_with_opts(&image_with_overlay, &options)?;
            Ok(buffer)
        }
        ImageFormat::Jpeg => {
            let options = get_jpeg_options(image_props.quality);
            let buffer = ops::jpegsave_buffer_with_opts(&image_with_overlay, &options)?;
            Ok(buffer)
        }
    }
}

fn get_webp_options(quality: u8) -> ops::WebpsaveBufferOptions {
    ops::WebpsaveBufferOptions {
        // Quality
        q: quality.into(),
        // Preset for lossy compression
        preset: ops::ForeignWebpPreset::Photo,
        // Strip all metadata from image
        strip: true,
        // Default values
        ..ops::WebpsaveBufferOptions::default()
    }
}

fn get_jpeg_options(quality: u8) -> ops::JpegsaveBufferOptions {
    ops::JpegsaveBufferOptions {
        // Quality
        q: quality.into(),
        // Strip all metadata from image
        strip: true,
        // Default values
        ..ops::JpegsaveBufferOptions::default()
    }
}

// Generate HTTP headers for the image.
fn get_headers(props: &ImageProps, image_id: &str, image_hash: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();

    let ext = props.format.to_string();
    let filename = match props.filename.clone() {
        Some(filename) => filename,
        None => format!("{image_hash}.{ext}"),
    };

    headers.insert(
        header::CONTENT_TYPE,
        format!("image/{ext}").parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("inline; filename=\"{filename}\"").parse().unwrap(),
    );
    headers.insert(header::ETAG, image_id.parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "max-age=604800".parse().unwrap());

    headers
}
