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
use std::{cmp, collections::HashMap, fmt, path::PathBuf, str::FromStr, sync::Arc};

#[derive(Debug)]
pub enum SupportedImageFormat {
    Webp,
    Jpeg,
}

impl fmt::Display for SupportedImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SupportedImageFormat::Jpeg => "jpeg",
                SupportedImageFormat::Webp => "webp",
            }
        )
    }
}

#[derive(Debug)]
pub struct ImageProps {
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub quality: Option<u8>,
    pub watermark: bool,
    // Supported values: webp (default), jpg
    pub format: SupportedImageFormat,
}

impl ImageProps {
    /// Parse URL parameters.
    fn from_params(params: &HashMap<String, String>) -> ImageProps {
        ImageProps {
            width: Self::parse_param(params.get("width")),
            height: Self::parse_param(params.get("height")),
            quality: Self::parse_param(params.get("quality")),
            watermark: params.get("watermark").is_some(),
            format: match params.get("format") {
                Some(val) => match val.as_str() {
                    "jpg" | "jpeg" => SupportedImageFormat::Jpeg,
                    "webp" => SupportedImageFormat::Webp,
                    _ => SupportedImageFormat::Webp,
                },
                None => SupportedImageFormat::Webp,
            },
        }
    }

    /// Convert optional parameter to number, ignore all errors.
    fn parse_param<T>(param: Option<&String>) -> Option<T>
    where
        T: FromStr,
    {
        match param {
            Some(val) => match val.parse::<T>() {
                Ok(res) => Some(res),
                Err(_) => None,
            },
            None => None,
        }
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
    let response_headers = get_headers(&image_props.format, image_id.clone());
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
        "{}-{}-{}-{}-{}-{}",
        hash,
        props.width.unwrap_or(0),
        props.height.unwrap_or(0),
        props.quality.unwrap_or(0),
        props.watermark,
        props.format,
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

    // Crop image if needed.
    let cropped_image = match image_props.width.is_some() || image_props.height.is_some() {
        // The user specified a height or width
        true => {
            // Resize the image so that the smaller side of the image is fully visible
            let original_width = rotated_image.get_width();
            let original_height = rotated_image.get_height();

            let width_scale_factor: f64 = match image_props.width {
                Some(desired_width) => f64::from(desired_width) / f64::from(original_width),
                None => 1.0,
            };
            let height_scale_factor: f64 = match image_props.height {
                Some(desired_height) => f64::from(desired_height) / f64::from(original_height),
                None => 1.0,
            };

            let min_factor = width_scale_factor.max(height_scale_factor).min(1.0);
            let resized_image = ops::resize(&rotated_image, min_factor)?;

            // Crop big side with smart algorithm
            ops::smartcrop(
                &resized_image,
                match image_props.width {
                    Some(val) => cmp::min(val.into(), resized_image.get_width()),
                    None => resized_image.get_width(),
                },
                match image_props.height {
                    Some(val) => cmp::min(val.into(), resized_image.get_height()),
                    None => resized_image.get_height(),
                },
            )?
        }
        false => rotated_image,
    };

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

    // Encode image.
    match image_props.format {
        SupportedImageFormat::Webp => {
            let options = get_webp_options(image_props.quality);
            let buffer = ops::webpsave_buffer_with_opts(&image_with_watermark, &options)?;
            Ok(buffer)
        }
        SupportedImageFormat::Jpeg => {
            let options = get_jpeg_options(image_props.quality);
            let buffer = ops::jpegsave_buffer_with_opts(&image_with_watermark, &options)?;
            Ok(buffer)
        }
    }
}

fn get_webp_options(quality: Option<u8>) -> ops::WebpsaveBufferOptions {
    ops::WebpsaveBufferOptions {
        // Quality
        q: quality.unwrap_or(80).into(),
        // Preset for lossy compression
        preset: ops::ForeignWebpPreset::Photo,
        // Strip all metadata from image
        strip: true,
        // Default values
        ..ops::WebpsaveBufferOptions::default()
    }
}

fn get_jpeg_options(quality: Option<u8>) -> ops::JpegsaveBufferOptions {
    ops::JpegsaveBufferOptions {
        // Quality
        q: quality.unwrap_or(80).into(),
        // Strip all metadata from image
        strip: true,
        // Default values
        ..ops::JpegsaveBufferOptions::default()
    }
}

// Generate HTTP headers for the image.
fn get_headers(format: &SupportedImageFormat, image_id: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        match format {
            SupportedImageFormat::Webp => "image/webp".parse().unwrap(),
            SupportedImageFormat::Jpeg => "image/jpeg".parse().unwrap(),
        },
    );
    headers.insert(header::ETAG, image_id.parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "max-age=604800".parse().unwrap());
    headers
}
