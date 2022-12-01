use clap::{arg, Command};

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
}

fn main() {
    

}
