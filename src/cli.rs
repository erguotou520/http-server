use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Copy, Debug, Clone)]
enum RunMode {
    // 默认
    Default,
    // SPA模式
    SPA,
    // 目录列表模式
    Index
}


#[derive(Parser, Clone)]
#[command(author, version, about, long_about = None)]
pub struct CliOption {
    /// Force enable file index
    #[arg(short = 'm', long, default_value = RunMode::Default)]
    pub mode: RunMode,

    /// Config file path
    #[arg(short = 'f', long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Host to listen on
    #[arg(short = 'H', long, value_name = "HOST", default_value_t = String::from("0.0.0.0"))]
    pub host: String,

    /// Port to listen on
    #[arg(short = 'p', long, value_name = "PORT", default_value_t = 8080)]
    pub port: u16,

    /// Enable gzip
    #[arg(short = 'g', long, value_name = "GZIP", default_value_t = true)]
    pub gzip: bool,

    /// Automatically open the browser
    #[arg(short = 'o', long, value_name = "open", default_value_t = false)]
    pub open: bool,

    /// Cache duration for static files
    #[arg(short = 'c', long, value_name = "CACHE", default_value_t = String::from("1d"))]
    pub cache: String,

    /// Path to save log at
    #[arg(short = 'l', long, value_name = "LOG")]
    pub log: Option<String>,

    /// Enable upload
    #[arg(short = 'u', long, value_name = "UPLOAD", default_value_t = false)]
    pub upload: bool,

    /// Set username:password for basic auth
    #[arg(short = 's', long, value_name = "SECURITY")]
    pub security: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Update hs self
    Update {},
}
