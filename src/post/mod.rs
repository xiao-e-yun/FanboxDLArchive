pub mod file;

use std::{borrow::Cow, collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use crate::config::{Config, TransformMethod};
use chrono::{DateTime, Utc};
use console::style;
use file::FanboxDLFileMeta;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info, warn};
use post_archiver::{
    importer::{
        file_meta::{ImportFileMetaMethod, UnsyncFileMeta},
        post::UnsyncPost,
        UnsyncContent,
    },
    manager::{PostArchiverConnection, PostArchiverManager},
    AuthorId,
};
use rusqlite::Connection;
use serde_json::json;
use tokio::{
    fs::{self, copy, create_dir_all, hard_link, rename},
    sync::Semaphore, task::JoinSet,
};

pub async fn get_posts(
    path: PathBuf,
    author: AuthorId,
) -> Result<Vec<UnsyncPost>, Box<dyn std::error::Error>> {
    let groups = read_fanbox_dl_archive(path).await?;
    Ok(groups
        .into_iter()
        .map(|group| {
            let post = UnsyncPost::new(author);
            match group {
                FanboxDLPost::Ungroup(files) => post
                    .title("Fanbox archive".to_string())
                    .content(files.into_iter().map(UnsyncContent::file).collect()),
                FanboxDLPost::GroupByPlan(plan, files) => post
                    .title(format!("{}yen fanbox archive", plan))
                    .content(files.into_iter().map(UnsyncContent::file).collect()),
                FanboxDLPost::GroupByPost(date, name, files) => post
                    .title(name)
                    .content(files.into_iter().map(UnsyncContent::file).collect())
                    .updated(date)
                    .published(date),
            }
            .tags(vec!["fanbox-dl".to_string()])
        })
        .filter(|post| post.content.len() > 0)
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
    ) -> Result<Vec<UnsyncFileMeta>, Box<dyn std::error::Error>> {
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
    Ungroup(Vec<UnsyncFileMeta>),
    GroupByPlan(u32, Vec<UnsyncFileMeta>),
    GroupByPost(DateTime<Utc>, String, Vec<UnsyncFileMeta>),
}

pub async fn sync_posts(
    manager: &mut PostArchiverManager<Connection>,
    config: &Config,
    posts: Vec<UnsyncPost>,
) -> Result<(), Box<dyn std::error::Error>> {
    let main_style =
        ProgressStyle::with_template(" {prefix:.bold} {bar} {wide_msg}").unwrap();
    let secondly_style = ProgressStyle::with_template("   * {prefix:.bold} {wide_msg:.bold.dim}").unwrap();

    let multi = config.multi();
    let total = multi.add(ProgressBar::new(posts.len() as u64));

    let mut join_set = JoinSet::new();
    let semaphores = Arc::new(Semaphore::new(config.limit()));
    for post in posts {
        let manager = manager.transaction()?;
        let post_pb = multi.add(
            ProgressBar::new(post.content.len() as u64 + 1)
                .with_style(main_style.clone())
                .with_prefix(post.title.clone())
        );

        post_pb.set_message("syncing");
        let sync_pb = multi.insert_after(
            &post_pb,
            ProgressBar::new(0)
                .with_style(secondly_style.clone())
                .with_message("syncing")
                .with_prefix("post")
        );
        let (_, files) = post.sync(&manager)?;
        post_pb.inc(1);

        if let Some((path,_)) = files.first() {
            create_dir_all(path.parent().unwrap()).await.ok();
        }

        post_pb.set_message("transforming");
        let transform = config.transform();
        for (output, method) in files {
            let post_pb = post_pb.clone();
            let semaphores = semaphores.clone();
            let filename = output.file_name().unwrap().to_string_lossy().to_string();
            let file_pb = multi.insert_after(
                &sync_pb,
                ProgressBar::new(0)
                    .with_style(secondly_style.clone())
                    .with_prefix(filename)
                    .with_message("transforming")
            );
            
            join_set.spawn(async move {
                let _semaphore = semaphores.acquire().await.unwrap();
                file_pb.tick();

                let ImportFileMetaMethod::File(input) = method else {
                    unreachable!()
                };

                let error = match transform {
                    TransformMethod::Copy => copy(input, output).await.err(),
                    TransformMethod::Move => rename(input, output).await.err(),
                    TransformMethod::Hardlink => hard_link(input, output).await.err()
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
