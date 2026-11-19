use std::{error::Error, fs, path::PathBuf};

use log::{debug, info};

use crate::config::Config;

pub fn get_creators(config: &Config) -> Result<Vec<(String, PathBuf)>, Box<dyn Error>> {
    info!("Checking creators");
    let mut creators = vec![];
    for entry in fs::read_dir(config.input())?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !entry.file_type()?.is_dir() {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        creators.push((name, entry.path()));
    }
    creators.sort();

    let total = creators.len();
    creators.retain(|(c, _)| config.filter_creator(c));
    let filtered = creators.len();
    info!("Total: {total} creators");
    info!("Excluded: {} creators", total - filtered);
    info!("Included: {filtered} creators");
    info!("");

    Ok(creators)
}

pub fn display_creators(creators: &[(String, PathBuf)]) {
    if log::log_enabled!(log::Level::Info) {
        info!("== Creator =============");
        for (creator, _) in creators {
            info!(" {creator}");
        }
        info!("========================");
        info!("");
    }
}
