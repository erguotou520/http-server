use clap::{Parser, Subcommand};

#[derive(clap::ValueEnum, Copy, Clone, Debug)]
pub enum WorkMode {
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
    /// Work mode
    #[arg(short = 'm', long)]
    pub mode: Option<WorkMode>,

    /// Folder to serve
    #[arg(short = 'p', long, value_name = "PATH", default_value_t = String::from("."))]
    pub path: String,

    /// Base URL path
    #[arg(short = 'b', long, value_name = "BASE", default_value_t = String::from("/"))]
    pub base: String,

    /// Host to listen on
    #[arg(short = 'h', long, value_name = "HOST", default_value_t = String::from("0.0.0.0"))]
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
    #[arg(long, value_name = "LOG")]
    pub log: Option<String>,

    /// Path to save error log at
    #[arg(long, value_name = "ERROR_LOG")]
    pub error_log: Option<String>,

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
