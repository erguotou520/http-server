use chrono::prelude::DateTime;
use chrono::Local;
use std::fs::{self};
use std::net::IpAddr;
use std::path::PathBuf;

use actix_files as afs;
use actix_web::http::header::{ContentDisposition, DispositionType};
use actix_web::{get, App, Error, HttpRequest, HttpResponse, HttpServer};
use local_ip_address::list_afinet_netifas;

use crate::cli::Cli;

use askama::Template;

#[derive(Template)]
#[template(path = "list.html")]
struct ListTemplate {
    path: String,
    path_list: Vec<String>,
    parent_path: String,
    files: Vec<FileItem>,
}

struct FileItem {
    name: String,
    path: String,
    is_dir: bool,
    size: String,
    update_time: String,
}

#[get("/{filename:.*}")]
async fn handler(req: HttpRequest) -> Result<HttpResponse, Error> {
    let path: PathBuf = req.match_info().query("filename").parse().unwrap();

    let existed = path.try_exists().unwrap();
    if existed {
        let file = afs::NamedFile::open(&path)?;
        if file.metadata().is_dir() {
            let mut files: Vec<FileItem> = vec![];
            for file in fs::read_dir(&path)? {
                let file = file?;
                let file_path = String::from(file.path().to_str().unwrap());
                let name = String::from(file.file_name().to_str().unwrap());
                let modified = file.metadata()?.modified()?;
                let modified_local: DateTime<Local> = modified.into();
                let update_time = modified_local.format("%Y-%m-%d %H:%M:%S").to_string();
                if file.path().is_dir() {
                    files.push(FileItem {
                        name,
                        path: file_path,
                        is_dir: true,
                        size: String::from("0"),
                        update_time,
                    })
                } else {
                    files.push(FileItem {
                        name,
                        path: file_path,
                        is_dir: false,
                        size: format_file_size(file.metadata()?.len()),
                        update_time,
                    })
                }
            }
            let full_path = String::from(path.to_str().unwrap());
            let path_list: Vec<String> = full_path.split("/").map(|s| s.to_string()).collect();
            let mut parent_list = path_list.clone();
            parent_list.pop();
            let html = ListTemplate {
                path: full_path.clone(),
                path_list,
                parent_path: parent_list.join("/"),
                files,
            }
            .render()
            .unwrap();
            let response = HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(html);
            Ok(response)
        } else {
            let response =
                file.use_last_modified(true)
                    .set_content_disposition(ContentDisposition {
                        disposition: DispositionType::Inline,
                        parameters: vec![],
                    });
            Ok(response.into_response(&req))
        }
    } else {
        let response = HttpResponse::NotFound()
            .content_type("text/html; charset=utf-8")
            .body("404");
        Ok(response)
    }
}

fn format_file_size(file_size: u64) -> String {
    let mut converted_size = file_size as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];

    let mut unit_index = 0;
    while converted_size >= 1024.0 && unit_index < units.len() - 1 {
        converted_size /= 1024.0;
        unit_index += 1;
    }
    let size = (converted_size * 100.0).floor() / 100.0;

    format!("{} {}", size, units[unit_index])
}

pub async fn start_server(options: Cli) -> std::io::Result<()> {
    let server = HttpServer::new(|| App::new().service(handler));

    if let Ok(server) = server.bind(("0.0.0.0", options.port)) {
        print_all_host(options.port);
        server.run().await
    } else {
        panic!("port {} is in use.", options.port);
    }
}

fn print_all_host(port: u16) {
    let ifas = list_afinet_netifas().unwrap();

    for (_, ip) in ifas.iter() {
        if matches!(ip, IpAddr::V4(_)) {
            println!("  http://{:?}:{}", ip, port);
        }
    }
}
