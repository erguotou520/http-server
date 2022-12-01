use clap::Parser;

struct Args {
    name: String
    count: utf8
}

fn main() {
    let args = Args::parse();

    for _ in 0..args.count {
        println!("Hello {}!", args.name);

    }

}
