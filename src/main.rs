use std::path::PathBuf;

use clap::{arg, value_parser, Command};

use crate::server::start_server;

mod server;

fn cli() -> Command {
    Command::new("hs")
        .about("A http server with best practice for morden web application")
        .version("0.0.1")
        .author("erguotou")
        .allow_external_subcommands(true)
        .subcommand(Command::new("update").about("Update hs self"))
        .arg(arg!([entryPath] "The entry path to serve"))
        .arg(arg!(-c --config <FILE> "Config file path").value_parser(value_parser!(PathBuf)))
        .arg(arg!(-d --host <HOST> "Host to listen on").default_value("0.0.0.0"))
        .arg(arg!(-p --port <PORT> "Port to listen on").value_parser(value_parser!(u16)).default_value("8080"))
        .arg(arg!(-g --gzip "Enable gzip").value_parser(value_parser!(bool)).default_value("true"))
        .arg(arg!(-o --open "Automatically open the browser").value_parser(value_parser!(bool)).default_value("false"))
        .arg(arg!(-e --cache <CACHE> "Cache duration for static files").default_value("1d"))
        .arg(arg!(-l --log <PATH> "Path to save log at").default_value("./log"))
}

#[actix_web::main]
async fn main() {
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("update", _sub_matches)) => {
            println!("update");
        }
        Some((ext, sub_matches)) => {
            println!("{:?}, {:?}", ext, sub_matches)
        }
        None => {
            println!("serve");
            println!("config: {:?}", matches.get_one::<PathBuf>("config"));
            println!("port: {:?}", matches.get_one::<u16>("port"));
            println!("gzip: {:?}", matches.get_one::<bool>("gzip"));
            _ = start_server().await
        }
    }
}
