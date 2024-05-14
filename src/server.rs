use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorUnauthorized;
use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::middleware::HttpAuthentication;
use chrono::prelude::DateTime;
use chrono::Local;
use std::fs::read_dir;
use std::net::IpAddr;
use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::http::header::{ContentDisposition, DispositionType};
use actix_web::{get, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use local_ip_address::list_afinet_netifas;
use open::that;

use crate::cli::CliOption;

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

struct BasicAuthState {
    username: String,
    password: String,
}

#[get("/{filename:.*}")]
async fn handler(req: HttpRequest) -> Result<HttpResponse, Error> {
    // web::scope(path)
    let path: PathBuf = req.match_info().query("filename").parse().unwrap();

    let existed = path.try_exists().unwrap();
    if existed {
        let file = NamedFile::open(&path)?;
        if file.metadata().is_dir() {
            if let Ok(response) = auto_render_index_html(path.clone()) {
                Ok(response.into_response(&req))
            } else {
                render_dir_index(path)
            }
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
        if let Ok(response) = auto_render_index_html(path.clone()) {
            Ok(response.into_response(&req))
        } else {
            let response = HttpResponse::NotFound()
                .content_type("text/html; charset=utf-8")
                .body("404");
            Ok(response)
        }
    }
}

fn render_dir_index(path: PathBuf) -> Result<HttpResponse, Error> {
    let mut files: Vec<FileItem> = vec![];
    // 遍历目录
    for file in read_dir(&path)? {
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
    // 全路径
    let full_path = String::from(path.to_str().unwrap());
    // 路径按照/分隔
    let path_list: Vec<String> = full_path.split("/").map(|s| s.to_string()).collect();
    // 上级路径
    let mut parent_list = path_list.clone();
    parent_list.pop();

    // 渲染页面
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

// 如果路径下有index.html，则直接返回index.html
fn auto_render_index_html(path: PathBuf) -> Result<NamedFile, bool> {
    // 拼接 index.html 路径
    let index_path = path.join("index.html");
    if index_path.exists() {
        if let Ok(file) = NamedFile::open(&index_path) {
            let response =
                file.use_last_modified(true)
                    .set_content_disposition(ContentDisposition {
                        disposition: DispositionType::Inline,
                        parameters: vec![],
                    });
            return Ok(response);
        }
    }
    Err(false)
}

pub async fn basic_auth(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let username = credentials.user_id();
    
    let state = req.app_data::<BasicAuthState>();
    match state {
        None => Ok(req),
        Some(state) => {
            // 没有指定用户名时
            if state.username.is_empty() {
                return Ok(req);
            }
            // 验证用户名和密码
            if username == state.username && credentials.password().unwrap() == state.password {
                Ok(req)
            } else {
                // 错误
                Err((ErrorUnauthorized("Invalid Credential"), req))
            }
        }
    }
}

pub async fn start_server(options: &CliOption) -> std::io::Result<()> {
    // 获取base url
    let base = options.base.clone();

    // 获取文件路径
    let root_path = options.path.clone();
    let log_path = options.log.clone();
    let security = options.security.clone();

    // let validator = |
    //     req: ServiceRequest,
    //     _credentials: BasicAuth,
    // | async {
    //     let user_id = _credentials.user_id().to_string();
    //     let password = _credentials.password().unwrap().to_string();

    //     if user_id == String::from("admin") && password == String::from("admin") {
    //         Ok(req)
    //     } else {
    //         // 错误
    //         Err((ErrorUnauthorized("Invalid Credential"), req))
    //     }
    // }
    let server = HttpServer::new(move || {
        let mut _user_id = String::new();
        let mut _password = String::new();
        if let Some(security) = &security {
            let parts: Vec<&str> = security.split(',').collect();
            if parts.len() != 2 {
                panic!("Error when parse basic auth")
            }
            _user_id = parts[0].to_string();
            _password = parts[1].to_string();
        }
        App::new()
            .app_data(web::Data::new(BasicAuthState {
                username: _user_id.to_string(),
                password: _password.to_string(),
            }))
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .wrap(HttpAuthentication::basic(basic_auth))
            // TODO basic auth 类型一直有问题
            // .wrap(HttpAuthentication::basic(move |req, credentials| async {
            //     if user_id.is_empty() {
            //         Ok(req)
            //     } else {
            //         if credentials.user_id().eq(user_id)
            //             && credentials.password().unwrap().eq(password)
            //         {
            //             Ok(req)
            //         } else {
            //             let config = req
            //                 .app_data::<Config>()
            //                 .cloned()
            //                 .unwrap_or_default();
            //             Err((
            //                 actix_web::Error::from(AuthenticationError::from(config)),
            //                 req,
            //             ))
            //         }
            //     }
            // }))
            .service(web::scope(&base).service(handler))
    });

    let host = options.host.as_str();

    if let Ok(server) = server.bind((host, options.port)) {
        print_all_host(host, options.port, options.open);
        server.run().await
    } else {
        panic!("port {} is in use.", options.port);
    }
}

fn print_all_host(host: &str, port: u16, open: bool) {
    if host.eq("0.0.0.0") {
        let ifas = list_afinet_netifas().unwrap();

        for (_, ip) in ifas.iter() {
            if matches!(ip, IpAddr::V4(_)) {
                println!("  http://{:?}:{}", ip, port);
            }
        }
        if open && !ifas.is_empty() {
            let _ = that(format!("http://{:?}:{}", ifas[0].1, port));
        }
    } else {
        println!("  http://{}:{}", host, port);
        if open {
            let _ = that(format!("http://{}:{}", host, port));
        }
    }
}
