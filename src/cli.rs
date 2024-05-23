use clap::{Parser, Subcommand};

#[derive(clap::ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum WorkMode {
    // 默认
    Default,
    // SPA模式
    SPA,
    // 目录列表模式
    Index,
}

#[derive(Parser, Clone)]
#[command(author, version, about, long_about = None)]
pub struct CliOption {
    /// Work mode
    #[arg(short = 'm', long, value_enum, default_value_t = WorkMode::Index)]
    pub mode: WorkMode,

    /// Folder to serve
    #[arg(short = 'f', long, value_name = "PATH", default_value_t = String::from("."))]
    pub path: String,

    /// Base URL path
    #[arg(short = 'b', long, value_name = "BASE", default_value_t = String::from(""))]
    pub base: String,

    /// Host to listen on
    #[arg(long, value_name = "HOST", default_value_t = String::from("0.0.0.0"))]
    pub host: String,

    /// Port to listen on
    #[arg(long, value_name = "PORT", default_value_t = 8080)]
    pub port: u16,

    /// Enable compress
    #[arg(
        short = 'c',
        long,
        value_name = "COMPRESS",
        action,
        default_value_t = true
    )]
    pub compress: bool,

    /// Automatically open the browser
    #[arg(short = 'o', long, value_name = "open", default_value_t = false)]
    pub open: bool,

    /// Cache duration for static files
    #[arg(long, value_name = "CACHE", default_value_t = true)]
    pub cache: bool,

    /// Path to save log at
    #[arg(long, value_name = "LOG")]
    pub log: Option<String>,

    /// Path to save error log at
    #[arg(long, value_name = "ERROR_LOG")]
    pub error_log: Option<String>,

    /// Enable upload, recommend to enable this in Index mode
    #[arg(short = 'u', long, value_name = "UPLOAD", default_value_t = false)]
    pub upload: bool,

    /// Set username:password for basic auth
    #[arg(short = 's', long, value_name = "SECURITY")]
    pub security: Option<String>,

    /// Custom 404 page url, eg: 404.html
    #[arg(long, value_name = "CUSTOM-404")]
    pub custom_404: Option<String>,

    /// Set proxy for requests, eg: /api->http://127.0.0.1:8080
    #[arg(short = 'P', long, value_name = "PROXY", num_args(0..))]
    pub proxies: Vec<String>,

    /// Set proxy for websocket, eg: /ws->http://127.0.0.1:5000
    #[arg(short = 'W', long, value_name = "WEBSOCKET-PROXY", num_args(0..))]
    pub websocket_proxies: Vec<String>,

    /// files to ignore, support regex
    #[arg(long, value_name = "IGNORE-FILES", default_value_t = String::from(r"^\."))]
    pub ignore_files: String,

    #[arg(long, value_name = "DISABLE-POWERED-BY", default_value_t = true)]
    pub disable_powered_by: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Update hs self
    Update {},
}
