use std::net::IpAddr;
use std::path::PathBuf;

use actix_files as fs;
use actix_web::http::header::{ContentDisposition, DispositionType};
use actix_web::{App, HttpRequest, HttpServer, get, Error};
use local_ip_address::list_afinet_netifas;

use crate::cli::Cli;

#[get("/{filename:.*}")]
async fn index(req: HttpRequest) -> Result<fs::NamedFile, Error> {
    let path: PathBuf = req.match_info().query("filename").parse().unwrap();
    let file = fs::NamedFile::open(path)?;
    Ok(file
        .use_last_modified(true)
        .set_content_disposition(ContentDisposition {
            disposition: DispositionType::Attachment,
            parameters: vec![],
        }))
}

pub async fn start_server(options: Cli) -> std::io::Result<()> {
    let server = HttpServer::new(|| {
        App::new().service(index)
    })
        .bind(("0.0.0.0", options.port))?;
    print_all_host(options.port);
    
    server.run()
        .await
}

fn print_all_host(port: u16) {
    let ifas = list_afinet_netifas().unwrap();

    for (_, ip) in ifas.iter() {
        if matches!(ip, IpAddr::V4(_)) {
            println!("  http://{:?}:{}", ip, port);
        }
    }
}