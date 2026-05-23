use std::path::PathBuf;
use tempfile::Builder;

#[derive(Debug)]
pub enum PasteImageError {
    ClipboardUnavailable(String),
    NoImage(String),
    DecodeFailed(String),
    EncodeFailed(String),
    IoError(String),
}

impl std::fmt::Display for PasteImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteImageError::ClipboardUnavailable(msg) => write!(f, "clipboard unavailable: {msg}"),
            PasteImageError::NoImage(msg) => write!(f, "no image on clipboard: {msg}"),
            PasteImageError::DecodeFailed(msg) => write!(f, "could not decode image: {msg}"),
            PasteImageError::EncodeFailed(msg) => write!(f, "could not encode image: {msg}"),
            PasteImageError::IoError(msg) => write!(f, "io error: {msg}"),
        }
    }
}
impl std::error::Error for PasteImageError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat {
    Png,
}

#[derive(Debug, Clone)]
pub struct PastedImageInfo {
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub encoded_format: EncodedImageFormat,
}

/// Capture image from system clipboard, encode to PNG, and return bytes plus metadata.
pub fn paste_image_as_png() -> Result<(Vec<u8>, PastedImageInfo), PasteImageError> {
    tracing::debug!("attempting clipboard image read");
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| PasteImageError::ClipboardUnavailable(e.to_string()))?;
    let img = cb
        .get_image()
        .map_err(|e| PasteImageError::NoImage(e.to_string()))?;
    let w = img.width as u32;
    let h = img.height as u32;

    let mut png: Vec<u8> = Vec::new();
    let Some(rgba_img) = image::RgbaImage::from_raw(w, h, img.bytes.into_owned()) else {
        return Err(PasteImageError::EncodeFailed("invalid RGBA buffer".into()));
    };
    let dyn_img = image::DynamicImage::ImageRgba8(rgba_img);
    tracing::debug!("clipboard image decoded RGBA {w}x{h}");
    {
        let mut cursor = std::io::Cursor::new(&mut png);
        dyn_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| PasteImageError::EncodeFailed(e.to_string()))?;
    }

    tracing::debug!(
        "clipboard image encoded to PNG ({len} bytes)",
        len = png.len()
    );
    Ok((
        png,
        PastedImageInfo { width: w, height: h, encoded_format: EncodedImageFormat::Png },
    ))
}

/// Write clipboard PNG to a temporary file and return the path.
pub fn paste_image_to_temp_png() -> Result<(PathBuf, PastedImageInfo), PasteImageError> {
    let (png, info) = paste_image_as_png()?;
    let tmp = Builder::new()
        .prefix("codex-clipboard-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| PasteImageError::IoError(e.to_string()))?;
    std::fs::write(tmp.path(), &png).map_err(|e| PasteImageError::IoError(e.to_string()))?;
    let (_file, path) = tmp.keep().map_err(|e| PasteImageError::IoError(e.error.to_string()))?;
    Ok((path, info))
}

/// Interpret pasted text as an image (data URL or raw base64) and write a PNG temporary file.
pub fn try_decode_base64_image_to_temp_png(pasted: &str) -> Result<(PathBuf, PastedImageInfo), PasteImageError> {
    let s = pasted.trim();
    if s.is_empty() {
        return Err(PasteImageError::DecodeFailed("empty".into()));
    }

    let (_maybe_mime, b64) = if let Some(rest) = s.strip_prefix("data:") {
        if let Some(idx) = rest.find(',') {
            let (head, tail) = rest.split_at(idx);
            if !head.contains(";base64") {
                return Err(PasteImageError::DecodeFailed("data URL without base64".into()));
            }
            (Some(head.split(';').next().unwrap_or("").to_string()), &tail[1..])
        } else {
            return Err(PasteImageError::DecodeFailed("malformed data URL".into()));
        }
    } else {
        let looks_imagey = s.starts_with("iVBORw0K")
            || s.starts_with("/9j/")
            || s.starts_with("R0lGODlh")
            || s.starts_with("R0lGODdh");
        if !looks_imagey {
            return Err(PasteImageError::DecodeFailed("not image-like base64".into()));
        }
        (None, s)
    };

    let compact: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|e| PasteImageError::DecodeFailed(e.to_string()))?;

    let dyn_img = image::load_from_memory(&bytes)
        .map_err(|e| PasteImageError::DecodeFailed(e.to_string()))?;
    let (w, h) = (dyn_img.width(), dyn_img.height());

    let mut png: Vec<u8> = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png);
        dyn_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| PasteImageError::EncodeFailed(e.to_string()))?;
    }

    let tmp = Builder::new()
        .prefix("codex-clipboard-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| PasteImageError::IoError(e.to_string()))?;
    std::fs::write(tmp.path(), &png).map_err(|e| PasteImageError::IoError(e.to_string()))?;
    let (_file, path) = tmp.keep().map_err(|e| PasteImageError::IoError(e.error.to_string()))?;

    tracing::debug!("decoded pasted base64 image to {w}x{h} PNG at {}", path.to_string_lossy());
    Ok((path, PastedImageInfo { width: w, height: h, encoded_format: EncodedImageFormat::Png }))
}

/// Normalize pasted text that may represent a filesystem path.
pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    let pasted = pasted.trim();

    if let Ok(url) = url::Url::parse(pasted) {
        if url.scheme() == "file" {
            return url.to_file_path().ok();
        }
    }

    let looks_like_windows_path = {
        let drive = pasted
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
            && pasted.get(1..2) == Some(":")
            && pasted
                .get(2..3)
                .map(|s| s == "\\" || s == "/")
                .unwrap_or(false);
        let unc = pasted.starts_with("\\\\");
        drive || unc
    };
    if looks_like_windows_path {
        return Some(PathBuf::from(pasted));
    }

    let parts: Vec<String> = shlex::Shlex::new(pasted).collect();
    if parts.len() == 1 {
        return parts.into_iter().next().map(PathBuf::from);
    }

    None
}

// Image format inference removed alongside clipboard image helpers.

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use base64::Engine as _;

    /// Build a 1×1 red RGBA PNG in memory for test fixtures.
    fn tiny_png_bytes() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let dyn_img = image::DynamicImage::ImageRgba8(img);
        let mut buf = Vec::new();
        dyn_img
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .expect("encode test PNG");
        buf
    }

    // ── try_decode_base64_image_to_temp_png ──────────────────────────────────

    #[test]
    fn base64_raw_png_roundtrip() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png_bytes());
        assert!(b64.starts_with("iVBORw0K"), "raw base64 of PNG must start with iVBORw0K");
        let (path, info) = try_decode_base64_image_to_temp_png(&b64).expect("decode ok");
        assert_eq!((info.width, info.height), (1, 1));
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn base64_data_url_png_roundtrip() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png_bytes());
        let data_url = format!("data:image/png;base64,{b64}");
        let (path, info) = try_decode_base64_image_to_temp_png(&data_url).expect("decode ok");
        assert_eq!((info.width, info.height), (1, 1));
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn base64_empty_is_decode_error() {
        assert!(matches!(
            try_decode_base64_image_to_temp_png(""),
            Err(PasteImageError::DecodeFailed(_))
        ));
    }

    #[test]
    fn base64_non_image_base64_is_decode_error() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"hello world");
        assert!(matches!(
            try_decode_base64_image_to_temp_png(&b64),
            Err(PasteImageError::DecodeFailed(_))
        ));
    }

    #[test]
    fn base64_data_url_without_base64_marker_is_decode_error() {
        assert!(matches!(
            try_decode_base64_image_to_temp_png("data:image/png,somedata"),
            Err(PasteImageError::DecodeFailed(_))
        ));
    }

    // ── normalize_pasted_path ────────────────────────────────────────────────

    #[test]
    fn normalize_file_url() {
        assert_eq!(
            normalize_pasted_path("file:///tmp/image.png"),
            Some(std::path::PathBuf::from("/tmp/image.png"))
        );
    }

    #[test]
    fn normalize_simple_unix_path() {
        assert_eq!(
            normalize_pasted_path("/home/user/photo.jpg"),
            Some(std::path::PathBuf::from("/home/user/photo.jpg"))
        );
    }

    #[test]
    fn normalize_quoted_path_with_spaces() {
        assert_eq!(
            normalize_pasted_path(r#""/home/user/my photo.jpg""#),
            Some(std::path::PathBuf::from("/home/user/my photo.jpg"))
        );
    }

    #[test]
    fn normalize_two_paths_returns_none() {
        assert!(normalize_pasted_path("/path/a /path/b").is_none());
    }

    #[test]
    fn normalize_windows_drive_path() {
        assert!(normalize_pasted_path(r"C:\Users\user\image.png").is_some());
    }

    #[test]
    fn normalize_unc_path() {
        assert!(normalize_pasted_path(r"\\server\share\image.png").is_some());
    }

    // ── paste_image_as_png: graceful-failure smoke test ──────────────────────
    //
    // Regression guard for #565 (Wayland clipboard image paste): arboard must
    // return a typed PasteImageError rather than panicking when no clipboard
    // server is reachable (e.g., headless CI). On a live Wayland session the
    // success path is exercised instead.
    #[test]
    fn paste_image_returns_typed_error_when_no_display() {
        match paste_image_as_png() {
            Ok(_) => {}
            Err(PasteImageError::ClipboardUnavailable(_)) => {}
            Err(PasteImageError::NoImage(_)) => {}
            Err(e) => panic!("unexpected PasteImageError variant: {e}"),
        }
    }
}
