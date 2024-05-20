use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorUnauthorized;
use actix_web::middleware::Condition;
use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::middleware::HttpAuthentication;
use chrono::prelude::DateTime;
use chrono::Local;
use fancy_regex::Regex;
use std::fs::read_dir;
use std::net::IpAddr;
use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::http::header::{ContentDisposition, DispositionType, LOCATION};
use actix_web::{get, middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use local_ip_address::list_afinet_netifas;
use open::that;

use crate::cli::{CliOption, WorkMode};

use askama::Template;

#[derive(Template)]
#[template(path = "list.html")]
struct ListTemplate {
    base_url: String,
    current_path: String,
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

struct AppState {
    base_url: String,
    username: String,
    password: String,
    path: PathBuf,
    mode: WorkMode,
    cache: bool,
    ignore_pattern: Regex,
    custom_404_url: String,
}

#[get("{filename:.*}")]
async fn handler(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let _file_path: PathBuf = req.match_info().query("filename").parse().unwrap();
    let prefix_url = state.base_url.clone();
    // 去掉前缀
    let _remove_prefix = _file_path.strip_prefix(&prefix_url);
    let file_path = if let Ok(path) = _remove_prefix {
        path.to_path_buf()
    } else {
        _file_path.to_path_buf()
    };
    // 对于忽略文件要屏蔽
    if !file_path.starts_with(".well-known") {
        if let Ok(_match) = state.ignore_pattern.is_match(&file_path.to_str().unwrap()) {
            if _match {
                return Ok(not_found_response(state));
            }
        }
    }
    let path = state.path.join(&file_path);
    let mode = state.mode;
    let existed = path.try_exists().unwrap();
    if existed {
        let file = NamedFile::open_async(&path).await?;
        // 目录
        if file.metadata().is_dir() {
            // 目录索引模式
            if mode == WorkMode::Index {
                return render_dir_index(
                    &state.base_url,
                    &state.path,
                    &file_path,
                    &state.ignore_pattern,
                );
            }
            // SPA 模式
            if mode == WorkMode::SPA {
                // try 返回 index.html
                if let Ok(response) = auto_render_index_html(&state.path) {
                    return Ok(response.into_response(&req));
                } else {
                    return Ok(not_found_response(state));
                }
            }
            // 默认模式 403
            return Ok(forbidden_response());
        } else {
            // 返回文件本身
            let mut response = file
                .prefer_utf8(true);
            if state.cache {
                response = response.use_etag(true).use_last_modified(true);
            }
            Ok(response.into_response(&req))
        }
    } else {
        // 文件不存在
        // 如果是 spa 模式
        if mode == WorkMode::SPA {
            // try 返回 index.html
            if let Ok(response) = auto_render_index_html(&state.path) {
                return Ok(response.into_response(&req));
            } else {
                return Ok(not_found_response(state));
            }
        }
        Ok(not_found_response(state))
    }
}

// 403
fn forbidden_response() -> HttpResponse {
    return HttpResponse::Forbidden()
        .content_type("text/html; charset=utf-8")
        .finish();
}

// 404
fn not_found_response(state: web::Data<AppState>) -> HttpResponse {
    let custom_404_url = state.custom_404_url.clone();
    if !custom_404_url.is_empty() {
        return HttpResponse::MovedPermanently()
            .insert_header((LOCATION, custom_404_url))
            .finish();
    }
    return HttpResponse::NotFound()
        .content_type("text/html; charset=utf-8")
        .finish();
}

fn render_dir_index(
    base_url: &str,
    base_path: &PathBuf,
    file_path: &PathBuf,
    ignore_pattern: &Regex,
) -> Result<HttpResponse, Error> {
    let mut files: Vec<FileItem> = vec![];

    // 遍历目录
    for file in read_dir(base_path.join(file_path))? {
        let file = file?;
        // 去掉base_path前缀
        let file_path =
            String::from(file.path().to_str().unwrap()).replace(base_path.to_str().unwrap(), "");
        let name = String::from(file.file_name().to_str().unwrap());
        // 忽略隐藏文件
        if !&name.starts_with(".well-known") {
            if let Ok(_match) = ignore_pattern.is_match(&name) {
                if _match {
                    continue;
                }
            }
        }
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

    // 排序，目录在前，文件在后
    files.sort_by(|a, b| {
        if a.is_dir && !b.is_dir {
            std::cmp::Ordering::Less
        } else if !a.is_dir && b.is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    // 路径按照/分隔
    let path_list: Vec<String> = file_path
        .to_string_lossy()
        .split("/")
        .map(|s| s.to_string())
        .collect();
    let parent_path = file_path.parent();
    // 渲染页面
    let html = ListTemplate {
        base_url: base_url.to_string(),
        // 去掉最后一个/
        current_path: String::from(file_path.to_str().unwrap()),
        path_list,
        parent_path: match parent_path {
            Some(p) => {
                if p.to_str().unwrap().is_empty() {
                    String::from("/")
                } else {
                    String::from(p.to_str().unwrap())
                }
            }
            None => String::from(""),
        },
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
fn auto_render_index_html(path: &PathBuf) -> Result<NamedFile, bool> {
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

async fn basic_auth(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let username = credentials.user_id();

    let state = req.app_data::<AppState>();
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
    // 获取base url，移除开头的/和结尾的/
    let mut base = options.base.clone();
    if base.starts_with("/") {
        base.remove(0);
    }
    if base.ends_with("/") {
        base.pop();
    }
    // 构建base url，没有前置/的添加
    let mut base_url: String = base.clone();
    if !base_url.starts_with("/") {
        base_url = format!("/{}", base_url);
    }
    // 是否开启压缩
    let compress = options.compress.clone();
    // 是否开启cache
    let cache = options.cache.clone();

    // 获取文件路径
    let root_path = options.path.clone();
    // let log_path = options.log.clone();
    let security = options.security.clone();
    let mode = options.mode;
    // 要忽略的文件
    let ignore_pattern = Regex::new(&options.ignore_files).unwrap();
    let custom_404_url: String = if let Some(url) = &options.custom_404 {
        url.to_string()
    } else {
        String::from("")
    };
    let server = HttpServer::new(move || {
        let mut _user_id = String::new();
        let mut _password = String::new();
        if let Some(security) = &security {
            let parts: Vec<&str> = security.split(':').collect();
            if parts.len() != 2 {
                panic!("Error when parse basic auth")
            }
            _user_id = parts[0].to_string();
            _password = parts[1].to_string();
        }
        App::new()
            .wrap(middleware::Logger::default())
            // TODO 大文件压缩效率、时间待优化
            .wrap(Condition::new(compress, middleware::Compress::default()))
            .wrap(Condition::new(
                !_user_id.is_empty(),
                HttpAuthentication::basic(basic_auth),
            ))
            // 使用actix-web的scope有问题，无法响应 /，只能响应子路由，所以手动处理
            // .service(handler)
            // .service(
            //     web::scope(if &base == "/" { "" } else { &base })
            .app_data(web::Data::new(AppState {
                base_url: base.clone(),
                username: _user_id.to_string(),
                password: _password.to_string(),
                path: PathBuf::from(&root_path),
                mode,
                cache,
                ignore_pattern: ignore_pattern.clone(),
                custom_404_url: custom_404_url.clone(),
            }))
            .service(handler)
        // )
    });

    let host = options.host.as_str();

    if let Ok(server) = server.bind((host, options.port)) {
        print_all_host(host, options.port, options.open, &base_url);
        server.run().await
    } else {
        panic!("port {} is in use.", options.port);
    }
}

fn print_all_host(host: &str, port: u16, open: bool, base: &str) {
    if host.eq("0.0.0.0") {
        let ifas = list_afinet_netifas().unwrap();

        for (_, ip) in ifas.iter() {
            if matches!(ip, IpAddr::V4(_)) {
                println!("  http://{:?}:{}{}", ip, port, base);
            }
        }
        if open && !ifas.is_empty() {
            let _ = that(format!("http://{:?}:{}{}", ifas[0].1, port, base));
        }
    } else {
        println!("  http://{}:{}{}", host, port, base);
        if open {
            let _ = that(format!("http://{}:{}{}", host, port, base));
        }
    }
}
