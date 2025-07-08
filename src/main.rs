mod config;
mod creator;
mod post;

use std::error::Error;

use config::Config;
use console::style;
use creator::{display_creators, get_creators, sync_creators};
use log::{info, warn};
use post::{get_posts, sync_posts};
use post_archiver::{manager::PostArchiverManager, utils::VERSION};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::parse();
    config.init_logger();

    info!("{}", style("Fanbox DL Archive").bold().dim());
    info!("");
    info!("==================================");
    info!("PostArchiver version: {}",style(format!("v{}",VERSION)).bold());
    info!("Overwrite: {}",style(config.overwrite()).bold());
    info!("Transform: {}",style(config.transform()).bold());
    info!("Input: {}",style(config.input().display()).bold());
    info!("Output: {}",style(config.output().display()).bold());
    info!("==================================");

    if !config.output().exists() {
        warn!("Creating output folder");
        std::fs::create_dir_all(config.output())?;
    }

    info!("Connecting to PostArchiver");
    let mut manager = PostArchiverManager::open_or_create(config.output())?;

    info!("Loading Creator List");
    let creators = get_creators(&config).await?;
    display_creators(&creators);

    let platform = manager.import_platform("fanbox-dl".to_string())?;

    info!("Syncing Creator List"); 
    let authors = sync_creators(&mut manager, creators, platform)?;

    info!("Resolve Creators Post");
    for (_, path) in authors {
        info!("* {}", style(&path.display()).bold());
        info!("resolving");
        let posts = get_posts(path, platform).await?;
        info!("");

        if !posts.is_empty() {
            info!("{} posts found", style(posts.len()).bold());
            info!("syncing");
            sync_posts(&mut manager, &config, posts).await?;
        }

        info!("");
    }

    info!("All done!");
    Ok(())
}
