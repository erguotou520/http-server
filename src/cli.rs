use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Config file path
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Host to listen on
    #[arg(short = 'd', long, value_name = "HOST")]
    pub host: Option<String>,

    /// Port to listen on
    #[arg(short = 'p', long, value_name = "PORT", default_value_t = 8080)]
    pub port: u16,    

    /// Enable gzip
    #[arg(short = 'g', long, default_value_t = true)]
    pub gzip: bool,

    /// Automatically open the browser
    #[arg(short = 'o', long, default_value_t = false)]
    pub open: bool,

    /// Cache duration for static files
    #[arg(short = 'e', long, value_name = "CACHE")]
    pub cache: Option<String>,

    /// Path to save log at
    #[arg(short = 'l', long, value_name = "LOG")]
    pub log: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Update hs self
    Update {
    },
}