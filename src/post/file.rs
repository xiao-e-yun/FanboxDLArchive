use std::{collections::HashMap, path::PathBuf};

use mime_guess::MimeGuess;
use post_archiver::importer::UnsyncFileMeta;
use serde_json::json;

/// Build file metadata for a local file, detecting its MIME type and image size.
pub fn file_meta_from_path(path: PathBuf) -> UnsyncFileMeta {
    let filename = path.file_name().unwrap().to_string_lossy().to_string();
    let mime = MimeGuess::from_path(&path)
        .first_or_octet_stream()
        .to_string();

    let mut extra = HashMap::new();
    if let Ok(size) = imagesize::size(&path) {
        extra.insert("width".to_string(), json!(size.width));
        extra.insert("height".to_string(), json!(size.height));
    }

    UnsyncFileMeta::new(filename, mime, path).extra(extra)
}
