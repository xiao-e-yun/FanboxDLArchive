pub mod file;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::config::Config;
use chrono::{DateTime, NaiveDate, Utc};
use file::file_meta_from_path;
use indicatif::ProgressBar;
use log::{debug, error, info, warn};
use post_archiver::{
    importer::{UnsyncAlias, UnsyncAuthor, UnsyncContent, UnsyncFileMeta, UnsyncPost, UnsyncTag},
    manager::PostArchiverManager,
};
use post_archiver_utils::Strategy;

pub const PLATFORM: &str = "fanbox-dl";

#[derive(Debug, Default, Clone, Copy)]
pub struct ImportStats {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Files of one fanbox post, keyed by (published date, title).
struct PostGroup {
    fee: Option<u32>,
    files: Vec<UnsyncFileMeta>,
}

/// Files that could not be matched to a post, per plan folder.
struct Leftover {
    fee: Option<u32>,
    files: Vec<UnsyncFileMeta>,
}

#[derive(Default)]
struct Scan {
    posts: BTreeMap<(DateTime<Utc>, String), PostGroup>,
    leftovers: Vec<Leftover>,
}

/// Read one creator folder written by fanbox-dl. Handles every layout:
///
/// - `creator/2006-01-02-<title>-[file-]<order>-<id>.<ext>`        (default)
/// - `creator/2006-01-02-<title>/[file-]<order>-<id>.<ext>`        (--dir-by-post)
/// - `creator/<fee>yen/...` with either of the above inside         (--dir-by-plan)
pub fn get_posts(creator: &str, path: &Path) -> std::io::Result<Vec<UnsyncPost>> {
    let mut scan = Scan::default();
    scan_level(path, None, &mut scan)?;

    let author = UnsyncAuthor::new(creator.to_string()).aliases(vec![UnsyncAlias::new(
        PLATFORM.to_string(),
        creator.to_string(),
    )
    .link(format!("https://{creator}.fanbox.cc/"))]);

    let base = path.to_string_lossy();
    let new_post =
        |source: String, title: String, mut files: Vec<UnsyncFileMeta>, fee: Option<u32>| {
            files.sort_by_cached_key(|f| (file_order(&f.filename), f.filename.clone()));
            let tags = fee
                .map(|fee| UnsyncTag {
                    name: format!("{fee}yen"),
                    platform: None,
                })
                .into_iter()
                .collect();
            UnsyncPost::new(
                PLATFORM.to_string(),
                source,
                title,
                files.into_iter().map(UnsyncContent::File).collect(),
            )
            .tags(tags)
            .authors(vec![author.clone()])
        };

    let mut posts = vec![];
    for ((date, title), group) in scan.posts {
        let source = format!("{base} - {}-{title}", date.format("%Y-%m-%d"));
        posts.push(
            new_post(source, title, group.files, group.fee)
                .published(date)
                .updated(date),
        );
    }
    // NOTE: leftover sources match v0.1, so re-imports update the old posts.
    for leftover in scan.leftovers.into_iter().filter(|l| !l.files.is_empty()) {
        let (source, title) = match leftover.fee {
            None => (base.to_string(), "Fanbox archive".to_string()),
            Some(fee) => (
                format!("{base} - {fee}yen"),
                format!("{fee}yen fanbox archive"),
            ),
        };
        posts.push(new_post(source, title, leftover.files, leftover.fee));
    }

    posts.retain(|post| !post.content.is_empty());
    Ok(posts)
}

fn scan_level(path: &Path, fee: Option<u32>, scan: &mut Scan) -> std::io::Result<()> {
    let mut leftover = vec![];

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            debug!(" ignoring: {}", entry.path().display());
            continue;
        }

        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            if let Some(fee) = fee.is_none().then(|| parse_plan_dir(&name)).flatten() {
                scan_level(&entry.path(), Some(fee), scan)?;
            } else if let Some((date, title)) = parse_dated(&name) {
                let files = read_dir_files(&entry.path(), 1)?;
                push_files(scan, date, title.to_string(), fee, files);
            } else {
                debug!(" ignoring: {}", entry.path().display());
            }
        } else if filetype.is_file() {
            let mut meta = file_meta_from_path(entry.path());
            match parse_flat_file(&name) {
                Some((date, title, filename)) => {
                    meta.filename = filename;
                    push_files(scan, date, title.to_string(), fee, vec![meta]);
                }
                None => leftover.push(meta),
            }
        } else {
            warn!(" {} is not a file or directory", entry.path().display());
        }
    }

    scan.leftovers.push(Leftover {
        fee,
        files: leftover,
    });
    Ok(())
}

fn push_files(
    scan: &mut Scan,
    date: DateTime<Utc>,
    title: String,
    fee: Option<u32>,
    files: Vec<UnsyncFileMeta>,
) {
    scan.posts
        .entry((date, title))
        .or_insert(PostGroup { fee, files: vec![] })
        .files
        .extend(files);
}

/// Parse `2006-01-02-<rest>`.
fn parse_dated(name: &str) -> Option<(DateTime<Utc>, &str)> {
    let date = name.get(..10)?;
    let rest = name.get(10..)?.strip_prefix('-')?;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((date.and_hms_opt(0, 0, 0)?.and_utc(), rest))
}

/// Parse `<fee>yen`.
fn parse_plan_dir(name: &str) -> Option<u32> {
    name.strip_suffix("yen")?.parse().ok()
}

/// Parse `2006-01-02-<title>-[file-]<order>-<id>.<ext>` into (date, title, `[file-]<order>-<id>.<ext>`).
/// Names truncated by fanbox-dl (ending in `...`) don't match and stay ungrouped.
fn parse_flat_file(name: &str) -> Option<(DateTime<Utc>, &str, String)> {
    let (date, rest) = parse_dated(name)?;
    let (stem, ext) = rest.rsplit_once('.')?;
    let mut parts = stem.rsplitn(3, '-');
    let id = parts
        .next()
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))?;
    let order: u32 = parts.next()?.parse().ok()?;
    let head = parts.next()?;
    let (title, prefix) = match head.strip_suffix("-file") {
        Some(title) => (title, "file-"),
        None => (head, ""),
    };
    Some((date, title, format!("{prefix}{order}-{id}.{ext}")))
}

/// Order of a `[file-]<order>-<id>.<ext>` file, for sorting post content.
fn file_order(filename: &str) -> u32 {
    let name = filename.strip_prefix("file-").unwrap_or(filename);
    name.split('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX)
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
