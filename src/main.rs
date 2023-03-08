mod server;
mod cli;

use clap::Parser;
use cli::{Cli, Commands};

use crate::server::start_server;

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Update {  }) => {
            println!("update");
        },
        None => {
            _ = start_server(cli).await
        }
    }
}
