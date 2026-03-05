pub mod file;

use std::path::PathBuf;

use crate::config::{Config, TransformMethod};
use chrono::{DateTime, Utc};
use console::style;
use file::FanboxDLFileMeta;
use indicatif::ProgressBar;
use log::{debug, info, warn};
use post_archiver::{
    importer::{post::UnsyncPost, UnsyncContent, UnsyncFileMeta},
    manager::PostArchiverManager,
    PlatformId,
};
use rusqlite::Connection;
use tokio::fs;

pub async fn get_posts(
    path: PathBuf,
    platform: PlatformId,
) -> Result<Vec<UnsyncPost<PathBuf>>, Box<dyn std::error::Error>> {
    fn to_contents(files: Vec<UnsyncFileMeta<PathBuf>>) -> Vec<UnsyncContent<PathBuf>> {
        files
            .iter()
            .map(|file| UnsyncContent::File(file.clone()))
            .collect()
    }

    let groups = read_fanbox_dl_archive(path.clone()).await?;

    Ok(groups
        .into_iter()
        .map(|group| match group {
            FanboxDLPost::Ungroup(files) => UnsyncPost::new(
                platform,
                path.to_string_lossy().to_string(),
                "Fanbox archive".to_string(),
                to_contents(files),
            ),
            FanboxDLPost::GroupByPlan(plan, files) => UnsyncPost::new(
                platform,
                format!("{} - {}yen", path.to_string_lossy(), plan),
                "{}yen fanbox archive".to_string(),
                to_contents(files),
            ),
            FanboxDLPost::GroupByPost(date, name, files) => UnsyncPost::new(
                platform,
                format!("{} - {}", path.to_string_lossy(), name),
                name,
                to_contents(files),
            )
            .published(date)
            .updated(date),
        })
        .filter(|post| !post.content.is_empty())
        .collect())
}

pub async fn read_fanbox_dl_archive(
    path: PathBuf,
) -> Result<Vec<FanboxDLPost>, Box<dyn std::error::Error>> {
    const MAX_DEPTH: usize = 5;
    let mut posts = vec![];
    let mut ungroup = vec![];

    let mut entrys = fs::read_dir(path).await?;
    while let Ok(Some(entry)) = entrys.next_entry().await {
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with('.') {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        let filetype = entry.file_type().await?;
        if filetype.is_dir() {
            let yen = filename.trim_end_matches("yen");
            let is_plan = yen != filename;
            if is_plan {
                let yen = yen.parse::<u32>()?;
                let files = read_dir_files(entry.path(), 1).await?;
                posts.push(FanboxDLPost::GroupByPlan(yen, files));
                continue;
            }

            let (date, name) = filename.split_at(11);
            let date = DateTime::parse_from_str(date, "%Y-%m-%d-").ok();
            if let Some(date) = date {
                let date = date.to_utc();
                let files = read_dir_files(entry.path(), 1).await?;
                posts.push(FanboxDLPost::GroupByPost(date, name.to_string(), files));
                continue;
            }

            debug!(" ignoring: {}", entry.path().display());
        } else if filetype.is_file() {
            ungroup.push(UnsyncFileMeta::from_path(entry.path()));
        } else {
            warn!(" {} is not a file or directory", entry.path().display());
        }
    }

    posts.push(FanboxDLPost::Ungroup(ungroup));

    #[async_recursion::async_recursion]
    async fn read_dir_files(
        path: PathBuf,
        level: usize,
    ) -> Result<Vec<UnsyncFileMeta<PathBuf>>, Box<dyn std::error::Error>> {
        let mut list = vec![];

        if level > MAX_DEPTH {
            warn!(" over expect depth {}", MAX_DEPTH);
            return Ok(list);
        }

        let mut dirs = vec![];

        let mut entrys = fs::read_dir(path).await?;
        while let Ok(Some(entry)) = entrys.next_entry().await {
            let filename = entry.file_name().to_string_lossy().to_string();
            if filename.starts_with('.') {
                debug!(" ignoring: {}", entry.path().display());
                continue;
            }

            let filetype = entry.file_type().await?;
            if filetype.is_dir() {
                dirs.push(read_dir_files(entry.path(), level + 1));
            } else if filetype.is_file() {
                list.push(UnsyncFileMeta::from_path(entry.path()));
            } else {
                warn!(" {} is not a file or directory", entry.path().display());
            }
        }

        for dir in dirs {
            let files = dir.await?;
            list.extend(files);
        }

        Ok(list)
    }

    Ok(posts)
}

pub enum FanboxDLPost {
    Ungroup(Vec<UnsyncFileMeta<PathBuf>>),
    GroupByPlan(u32, Vec<UnsyncFileMeta<PathBuf>>),
    GroupByPost(DateTime<Utc>, String, Vec<UnsyncFileMeta<PathBuf>>),
}

pub async fn sync_posts(
    manager: &mut PostArchiverManager<Connection>,
    config: &Config,
    posts: Vec<UnsyncPost<PathBuf>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let multi = config.multi();
    let total = multi.add(ProgressBar::new(posts.len() as u64));

    for post in posts {
        sync_post(manager, config, post)?;
        total.inc(1);
    }
    total.finish_and_clear();

    let success = total.position();
    let total = total.length().unwrap();

    info!("");
    info!("{} {}", total, style("total").dim());
    info!("{} {}", success, style("success").green());
    info!("{} {}", total - success, style("failed").red());
    info!("");
    Ok(())
}

fn sync_post(
    manager: &mut PostArchiverManager<Connection>,
    config: &Config,
    post: UnsyncPost<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let manager = manager.transaction()?;
    match config.transform() {
        TransformMethod::Copy => manager.import_post_with_files(post)?,
        TransformMethod::Move => manager.import_post_with_rename_files(post)?,
    };
    manager.commit()?;
    Ok(())
}
