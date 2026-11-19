pub mod file;

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;
use chrono::{DateTime, NaiveDate, Utc};
use file::file_meta_from_path;
use indicatif::ProgressBar;
use log::{debug, error, info, warn};
use post_archiver::{
    importer::{UnsyncAlias, UnsyncAuthor, UnsyncContent, UnsyncFileMeta, UnsyncPost},
    manager::PostArchiverManager,
};
use post_archiver_utils::Strategy;

pub const PLATFORM: &str = "fanbox-dl";

pub enum FanboxDLPost {
    Ungroup(Vec<UnsyncFileMeta>),
    GroupByPlan(u32, Vec<UnsyncFileMeta>),
    GroupByPost(DateTime<Utc>, String, Vec<UnsyncFileMeta>),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImportStats {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn get_posts(creator: &str, path: &Path) -> std::io::Result<Vec<UnsyncPost>> {
    let to_contents = |files: Vec<UnsyncFileMeta>| -> Vec<UnsyncContent> {
        files.into_iter().map(UnsyncContent::File).collect()
    };

    let author = UnsyncAuthor::new(creator.to_string()).aliases(vec![UnsyncAlias::new(
        PLATFORM.to_string(),
        creator.to_string(),
    )
    .link(format!("https://{creator}.fanbox.cc/"))]);

    // NOTE: `source` strings must stay identical to v0.1, otherwise re-imports duplicate posts.
    let base = path.to_string_lossy();
    let posts = read_fanbox_dl_archive(path)?
        .into_iter()
        .map(|group| match group {
            FanboxDLPost::Ungroup(files) => UnsyncPost::new(
                PLATFORM.to_string(),
                base.to_string(),
                "Fanbox archive".to_string(),
                to_contents(files),
            ),
            FanboxDLPost::GroupByPlan(plan, files) => UnsyncPost::new(
                PLATFORM.to_string(),
                format!("{base} - {plan}yen"),
                format!("{plan}yen fanbox archive"),
                to_contents(files),
            ),
            FanboxDLPost::GroupByPost(date, name, files) => UnsyncPost::new(
                PLATFORM.to_string(),
                format!("{base} - {name}"),
                name,
                to_contents(files),
            )
            .published(date)
            .updated(date),
        })
        .filter(|post| !post.content.is_empty())
        .map(|post| post.authors(vec![author.clone()]))
        .collect();

    Ok(posts)
}

/// Parse a `YYYY-MM-DD-<title>` directory name.
fn parse_post_dir(name: &str) -> Option<(DateTime<Utc>, &str)> {
    let date = name.get(..10)?;
    let title = name.get(10..)?.strip_prefix('-')?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((date.and_hms_opt(0, 0, 0)?.and_utc(), title))
}

/// Parse a `<fee>yen` directory name.
fn parse_plan_dir(name: &str) -> Option<u32> {
    name.strip_suffix("yen")?.parse().ok()
}

pub fn read_fanbox_dl_archive(path: &Path) -> std::io::Result<Vec<FanboxDLPost>> {
    let mut posts = vec![];
    let mut ungroup = vec![];

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with('.') {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            if let Some(yen) = parse_plan_dir(&filename) {
                let files = read_dir_files(&entry.path(), 1)?;
                posts.push(FanboxDLPost::GroupByPlan(yen, files));
            } else if let Some((date, name)) = parse_post_dir(&filename) {
                let files = read_dir_files(&entry.path(), 1)?;
                posts.push(FanboxDLPost::GroupByPost(date, name.to_string(), files));
            } else {
                debug!(" ignoring: {}", entry.path().display());
            }
        } else if filetype.is_file() {
            ungroup.push(file_meta_from_path(entry.path()));
        } else {
            warn!(" {} is not a file or directory", entry.path().display());
        }
    }

    posts.push(FanboxDLPost::Ungroup(ungroup));
    Ok(posts)
}

fn read_dir_files(path: &Path, level: usize) -> std::io::Result<Vec<UnsyncFileMeta>> {
    const MAX_DEPTH: usize = 5;
    let mut list = vec![];

    if level > MAX_DEPTH {
        warn!(" over expect depth {MAX_DEPTH}: {}", path.display());
        return Ok(list);
    }

    let mut dirs: Vec<PathBuf> = vec![];
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            dirs.push(entry.path());
        } else if filetype.is_file() {
            list.push(file_meta_from_path(entry.path()));
        } else {
            warn!(" {} is not a file or directory", entry.path().display());
        }
    }

    for dir in dirs {
        list.extend(read_dir_files(&dir, level + 1)?);
    }

    Ok(list)
}

pub fn import_posts(
    manager: &mut PostArchiverManager,
    config: &Config,
    posts: Vec<UnsyncPost>,
) -> ImportStats {
    let mut stats = ImportStats::default();
    let progress = config.multi().add(ProgressBar::new(posts.len() as u64));

    for post in posts {
        progress.inc(1);

        if matches!(config.strategy(), Strategy::Increment) && is_imported(manager, &post) {
            debug!("Skipping already imported post {}", post.source);
            stats.skipped += 1;
            continue;
        }

        let source = post.source.clone();
        match manager.importer().import_post(post) {
            Ok(_) => {
                debug!("Post imported: {source}");
                stats.imported += 1;
            }
            Err(e) => {
                error!("Failed to import post: {source}\n{e}");
                stats.failed += 1;
            }
        }
    }
    progress.finish_and_clear();

    info!(
        "{} imported, {} skipped, {} failed",
        stats.imported, stats.skipped, stats.failed
    );
    stats
}

/// A post counts as imported when it exists and already has every file.
fn is_imported(manager: &mut PostArchiverManager, post: &UnsyncPost) -> bool {
    let importer = manager.importer();
    let Ok(Some(id)) = importer.lookup_post(&post.source) else {
        return false;
    };
    post.content.iter().all(|content| match content {
        UnsyncContent::File(file) => matches!(importer.lookup_file_meta(id, file), Ok(Some(_))),
        UnsyncContent::Text(_) => true,
    })
}
