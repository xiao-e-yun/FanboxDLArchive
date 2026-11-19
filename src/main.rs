mod config;
mod creator;
mod post;

use std::{error::Error, process::ExitCode};

use config::Config;
use creator::{display_creators, get_creators};
use log::{error, info};
use post::{get_posts, import_posts, ImportStats};
use post_archiver::utils::VERSION;
use post_archiver_utils::{display_metadata, setup_database};

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let config = Config::parse();

    display_metadata!("Fanbox DL Archive" {
        "Version": format!("v{}", env!("CARGO_PKG_VERSION")),
        "PostArchiver Version": format!("v{VERSION}"),
        "Strategy": config.strategy(),
        "Whitelist": config.whitelist().join(", "),
        "Blacklist": config.blacklist().join(", "),
        "Input": config.input().display(),
        "Output": config.output().display(),
    });

    let mut manager = setup_database(config.output())?;

    let creators = get_creators(&config)?;
    display_creators(&creators);

    let mut total = ImportStats::default();
    for (creator, path) in creators {
        info!("* {creator}");
        let posts = match get_posts(&creator, &path) {
            Ok(posts) => posts,
            Err(e) => {
                error!("Failed to read {}: {e}", path.display());
                total.failed += 1;
                continue;
            }
        };
        info!("{} posts found", posts.len());

        let stats = import_posts(&mut manager, &config, posts);
        total.imported += stats.imported;
        total.skipped += stats.skipped;
        total.failed += stats.failed;
        info!("");
    }

    info!(
        "All done! {} imported, {} skipped, {} failed",
        total.imported, total.skipped, total.failed
    );
    Ok(if total.failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
