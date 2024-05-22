mod server;
mod cli;
mod proxy;

use clap::Parser;
use cli::{CliOption, Commands};

use crate::server::start_server;

#[actix_web::main]
async fn main() {
    let cli_option = CliOption::parse();
    match cli_option.command {
        Some(Commands::Update {  }) => {
            // TODO
            println!("update");
        },
        None => {
            let opt = &cli_option;
            _ = start_server(opt).await
        }
    }
}
