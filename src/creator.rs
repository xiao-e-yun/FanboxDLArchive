use std::{error::Error, fs, path::PathBuf};

use console::style;
use log::{debug, info};
use post_archiver::{importer::UnsyncAuthor, manager::PostArchiverManager, Author, Link};
use rusqlite::Connection;

use crate::config::Config;

pub async fn get_creators(config: &Config) -> Result<Vec<(String, PathBuf)>, Box<dyn Error>> {
    info!("Checking creators");
    let mut creators = vec![];
    for entry in fs::read_dir(&config.input())?.flat_map(|e| e) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        let filetype = entry.file_type()?;
        if !filetype.is_dir() {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        };

        creators.push((name, entry.path()));
    }

    let total = creators.len();
    info!("{} {}", total, style("total").bold());
    creators.retain(|(c, _)| config.filter_creator(c));
    let filtered = creators.len();
    info!("{} {}", filtered, style("included"));
    info!("{} {}", total - filtered, style("excluded").dim());
    info!("");

    Ok(creators)
}

pub fn display_creators(creators: &[(String, PathBuf)]) {
    if log::log_enabled!(log::Level::Info) {
        let mut creators: Vec<String> = creators.into_iter().map(|(c, _)| c.clone()).collect();
        creators.sort_by(|a, b| a.cmp(&b));

        info!("== Creator =============");

        for creator in creators.iter() {
            info!(" {}", creator);
        }

        info!("========================");
        info!("");
    }
}

pub fn sync_creators(
    manager: &mut PostArchiverManager<Connection>,
    creators: Vec<(String, PathBuf)>,
) -> Result<Vec<(Author, PathBuf)>, Box<dyn Error>> {
    let mut list = vec![];
    let manager = manager.transaction()?;

    for (creator, path) in creators {
        let alias = format!("fanbox:{}", creator);

        let author = match manager.check_author(&[alias.clone()])? {
            Some(id) => manager.get_author(&id),
            None => {
                let link = Link::new("fanbox", &format!("https://{}.fanbox.cc/", creator));
                UnsyncAuthor::new(creator.to_string())
                    .alias(vec![alias])
                    .links(vec![link])
                    .sync(&manager)
            }
        }?;

        list.push((author, path));
    }

    manager.commit()?;
    Ok(list)
}
