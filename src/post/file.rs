use std::{collections::HashMap, path::PathBuf};

use mime_guess::MimeGuess;
use post_archiver::importer::file_meta::UnsyncFileMeta;
use serde_json::json;

pub trait FanboxDLFileMeta
where
    Self: Sized,
{
    fn from_path(path: PathBuf) -> (Self, PathBuf);
}

impl FanboxDLFileMeta for UnsyncFileMeta {
    fn from_path(path: PathBuf) -> (Self, PathBuf) {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let mime = MimeGuess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        let mut extra: HashMap<String, serde_json::Value> = Default::default();

        if let Ok(size) = imagesize::size(&path) {
            extra.insert("width".to_string(), json!(size.width));
            extra.insert("height".to_string(), json!(size.height));
        }

        (Self {
            filename,
            mime,
            extra,
        }, path)
    }
}
