pub mod file;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::config::{Config, TransformMethod};
use chrono::{DateTime, Utc};
use console::style;
use file::FanboxDLFileMeta;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use post_archiver::{
    importer::{post::UnsyncPost, UnsyncContent, UnsyncFileMeta},
    manager::PostArchiverManager,
    PlatformId,
};
use rusqlite::Connection;
use tokio::{
    fs::{self, copy, create_dir_all, hard_link, rename},
    sync::Semaphore,
    task::JoinSet,
};

pub async fn get_posts(
    path: PathBuf,
    platform: PlatformId,
) -> Result<Vec<(UnsyncPost, HashMap<String, PathBuf>)>, Box<dyn std::error::Error>> {
    fn to_file_metas(files: &[(UnsyncFileMeta, PathBuf)]) -> Vec<UnsyncContent> {
        files
            .iter()
            .map(|(file, _)| UnsyncContent::File(file.clone()))
            .collect()
    }

    fn to_file_map(files: Vec<(UnsyncFileMeta, PathBuf)>) -> HashMap<String, PathBuf> {
        files
            .into_iter()
            .map(|(file, path)| (file.filename, path))
            .collect()
    }

    let groups = read_fanbox_dl_archive(path.clone()).await?;

    Ok(groups
        .into_iter()
        .map(|group| match group {
            FanboxDLPost::Ungroup(files) => (
                UnsyncPost::new(
                    platform,
                    path.to_string_lossy().to_string(),
                    "Fanbox archive".to_string(),
                    to_file_metas(&files),
                ),
                to_file_map(files),
            ),
            FanboxDLPost::GroupByPlan(plan, files) => (
                UnsyncPost::new(
                    platform,
                    format!("{} - {}yen", path.to_string_lossy(), plan),
                    "{}yen fanbox archive".to_string(),
                    to_file_metas(&files),
                ),
                to_file_map(files),
            ),
            FanboxDLPost::GroupByPost(date, name, files) => (
                UnsyncPost::new(
                    platform,
                    format!("{} - {}", path.to_string_lossy(), name),
                    name,
                    to_file_metas(&files),
                )
                .published(date)
                .updated(date),
                to_file_map(files),
            ),
        })
        .filter(|(post, _)| !post.content.is_empty())
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
    ) -> Result<Vec<(UnsyncFileMeta, PathBuf)>, Box<dyn std::error::Error>> {
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
    Ungroup(Vec<(UnsyncFileMeta, PathBuf)>),
    GroupByPlan(u32, Vec<(UnsyncFileMeta, PathBuf)>),
    GroupByPost(DateTime<Utc>, String, Vec<(UnsyncFileMeta, PathBuf)>),
}

pub async fn sync_posts(
    manager: &mut PostArchiverManager<Connection>,
    config: &Config,
    posts: Vec<(UnsyncPost, HashMap<String, PathBuf>)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let main_style = ProgressStyle::with_template(" {prefix:.bold} {bar} {wide_msg}").unwrap();
    let secondly_style =
        ProgressStyle::with_template("   * {prefix:.bold} {wide_msg:.bold.dim}").unwrap();

    let multi = config.multi();
    let total = multi.add(ProgressBar::new(posts.len() as u64));

    let mut join_set = JoinSet::new();
    let semaphores = Arc::new(Semaphore::new(config.limit()));
    for (post, files) in posts {
        let manager = manager.transaction()?;
        let post_pb = multi.add(
            ProgressBar::new(post.content.len() as u64 + 1)
                .with_style(main_style.clone())
                .with_prefix(post.title.clone()),
        );

        post_pb.set_message("syncing");
        let sync_pb = multi.insert_after(
            &post_pb,
            ProgressBar::new(0)
                .with_style(secondly_style.clone())
                .with_message("syncing")
                .with_prefix("post"),
        );
        let (_, files) = post.sync(&manager, files)?;
        post_pb.inc(1);

        if let Some((path, _)) = files.first() {
            create_dir_all(path.parent().unwrap()).await.ok();
        }

        post_pb.set_message("transforming");
        let transform = config.transform();
        for (target, source) in files {
            let post_pb = post_pb.clone();
            let semaphores = semaphores.clone();
            let filename = target.file_name().unwrap().to_string_lossy().to_string();
            let file_pb = multi.insert_after(
                &sync_pb,
                ProgressBar::new(0)
                    .with_style(secondly_style.clone())
                    .with_prefix(filename)
                    .with_message("transforming"),
            );

            join_set.spawn(async move {
                let _semaphore = semaphores.acquire().await.unwrap();
                file_pb.tick();

                let error = match transform {
                    TransformMethod::Copy => copy(source, target).await.err(),
                    TransformMethod::Move => rename(source, target).await.err(),
                    TransformMethod::Hardlink => hard_link(source, target).await.err(),
                };

                match error {
                    Some(err) => file_pb.finish_with_message(err.to_string()),
                    None => file_pb.finish_and_clear(),
                }

                post_pb.inc(1);
            });
        }

        manager.commit()?;
    }

    join_set.join_all().await;
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
