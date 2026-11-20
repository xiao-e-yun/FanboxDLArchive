use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use dotenv::dotenv;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use post_archiver_utils::Strategy;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Parser)]
pub struct Config {
    /// Your fanbox dl archive path
    #[clap(env = "INPUT")]
    input: PathBuf,
    /// Archive directory to save into
    #[arg(short, long, default_value = "./archive", env = "OUTPUT")]
    output: PathBuf,
    /// Archiving strategy (increment: skip posts whose files are all imported)
    #[arg(long, default_value_t)]
    strategy: Strategy,
    /// Only import these creators (folder names under <INPUT>)
    #[arg(short, long, num_args = 0..)]
    whitelist: Vec<String>,
    /// Skip these creators (folder names under <INPUT>)
    #[arg(short, long, num_args = 0..)]
    blacklist: Vec<String>,
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
    #[clap(skip)]
    multi: MultiProgress,
}

impl Config {
    /// Parse the configuration from the environment and command line arguments
    pub fn parse() -> Self {
        dotenv().ok();
        let config = <Self as Parser>::parse();
        config.init_logger();
        config
    }
    /// Create a logger with the configured verbosity level
    fn init_logger(&self) {
        let level = self.verbose.log_level_filter();
        let logger = env_logger::Builder::new()
            .filter_level(level)
            .format_target(false)
            .build();

        LogWrapper::new(self.multi.clone(), logger)
            .try_init()
            .unwrap();

        log::set_max_level(level);
    }
    pub fn input(&self) -> &Path {
        self.input.as_path()
    }
    pub fn output(&self) -> &Path {
        self.output.as_path()
    }
    pub fn strategy(&self) -> Strategy {
        self.strategy
    }
    pub fn whitelist(&self) -> &[String] {
        &self.whitelist
    }
    pub fn blacklist(&self) -> &[String] {
        &self.blacklist
    }
    pub fn filter_creator(&self, creator: &str) -> bool {
        let creator = creator.to_string();
        (self.whitelist.is_empty() || self.whitelist.contains(&creator))
            && !self.blacklist.contains(&creator)
    }
    pub fn multi(&self) -> &MultiProgress {
        &self.multi
    }
}
