use clap::{arg, Command, Arg};

fn cli() -> Command {
    Command::new("hs")
        .about("A http server with best practice for morden web application")
        .version("0.0.1")
        .author("erguotou")
        .allow_external_subcommands(true)
        .subcommand(
            Command::new("update")
                .about("Update hs self")

        )
        .arg(arg!([entryPath] "The entry path to serve"))
        .arg(Arg::new("host").short('h').long("host").default_value("0.0.0.0"))
        .arg(Arg::new("port").short('p').long("port").default_value(8080))
        .arg(Arg::new("gzip").short('g').long("gzip").default_value(true))
        .arg(Arg::new("open").short('o').long("open").default_value(false))
        .arg(Arg::new("cache").short('c').long("cache").default_value("1d"))
        .arg(Arg::new("log").short('l').long("log").default_value("./log"))
        
}

fn main() {
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some("update") => {
            println!("update")
        }
        Some((ext, sub_matches)) => {
            println!("{:?}, {:?}", ext, sub_matches)
        }
        None => {

        }
    }

}
