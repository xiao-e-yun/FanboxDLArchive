use clap::{arg, Parser, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use dotenv::dotenv;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Parser, Default)]
pub struct Config {
    /// Your fanbox dl path
    #[clap(env = "INPUT")]
    input: PathBuf,
    /// Which you path want to save
    #[arg(default_value = "./archive", env = "OUTPUT")]
    output: PathBuf,
    /// Overwrite existing files
    #[arg(short, long)]
    overwrite: bool,
    /// Transform method
    #[arg(short, long, default_value = "copy")]
    transform: TransformMethod,
    /// Whitelist of creator IDs
    #[arg(short, long, num_args = 0..)]
    whitelist: Vec<String>,
    /// Blacklist of creator IDs
    #[arg(short, long, num_args = 0..)]
    blacklist: Vec<String>,
    /// Limit the number of concurrent copys
    #[arg(short, long, default_value = "5")]
    limit: usize,
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
    #[clap(skip)]
    multi: MultiProgress,
}

impl Config {
    /// Parse the configuration from the environment and command line arguments
    pub fn parse() -> Self {
        dotenv().ok();
        <Self as Parser>::parse()
    }
    /// Create a logger with the configured verbosity level
    pub fn init_logger(&self) {
        let level = self.verbose.log_level_filter();
        let logger = env_logger::Builder::new()
            .filter_level(level.clone())
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
    pub fn overwrite(&self) -> bool {
        self.overwrite
    }
    pub fn transform(&self) -> TransformMethod {
        self.transform
    }
    pub fn output(&self) -> &PathBuf {
        &self.output
    }
    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn filter_creator(&self, creator: &String) -> bool {
        let mut accept = true;

        accept &= self.whitelist.is_empty() || self.whitelist.contains(creator);
        accept &= !self.blacklist.contains(creator);

        accept
    }
    pub fn multi(&self) -> &MultiProgress {
        &self.multi
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum TransformMethod {
    #[default]
    Copy,
    Move,
    Hardlink,
}

impl Display for TransformMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformMethod::Copy => write!(f, "Copy"),
            TransformMethod::Move => write!(f, "Move"),
            TransformMethod::Hardlink => write!(f, "Hardlink"),
        }
    }
}
