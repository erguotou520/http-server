use actix_web::dev::{Service, ServiceRequest};
use actix_web::error::ErrorUnauthorized;
use actix_web::middleware::Condition;
use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::middleware::HttpAuthentication;
use awc::Client;
use chrono::prelude::DateTime;
use chrono::Local;
use env_logger::Env;
use fancy_regex::{Captures, Regex};
use std::fs::read_dir;
use std::io::Read;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use log::{self, info};
use std::env;

use actix_files::NamedFile;
use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::http::header::{self, ContentDisposition, DispositionType};
use actix_web::{
    get, middleware, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use local_ip_address::list_afinet_netifas;
use open::that;

use crate::cli::{CliOption, WorkMode};
use crate::proxy::{forward_request, ws_forward_request, ProxyItem};

use askama::Template;

#[derive(Template)]
#[template(path = "list.html")]
struct ListTemplate {
    base_url: String,
    current_path: String,
    path_list: Vec<String>,
    parent_path: String,
    files: Vec<FileItem>,
    enable_upload: bool,
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
    root_path: PathBuf,
    mode: WorkMode,
    cache: bool,
    ignore_pattern: Regex,
    custom_404_url: String,
    enable_upload: bool,
}

#[derive(Debug, MultipartForm)]
struct UploadForm {
    #[multipart(rename = "files")]
    files: Vec<TempFile>,
    path: actix_multipart::form::text::Text<String>,
}

#[post("/_upload")]
async fn upload(
    MultipartForm(form): MultipartForm<UploadForm>,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    for f in form.files {
        let path = PathBuf::from(state.root_path.clone())
            .join(form.path.clone())
            .join(f.file_name.unwrap());
        f.file.persist(path).unwrap();
    }

    Ok(HttpResponse::Ok())
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
    let path = state.root_path.join(&file_path);
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
                    &state.root_path,
                    &file_path,
                    &state.ignore_pattern,
                    state.enable_upload,
                );
            }
            // SPA 模式
            if mode == WorkMode::SPA {
                // try 返回 index.html
                if let Ok(response) = auto_render_index_html(&state.root_path) {
                    return Ok(response.into_response(&req));
                } else {
                    return Ok(not_found_response(state));
                }
            }
            // 默认模式 403
            return Ok(forbidden_response());
        } else {
            // 返回文件本身
            let mut response = file.prefer_utf8(true);
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
            if let Ok(response) = auto_render_index_html(&state.root_path) {
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
        let _404_path = state.root_path.join(custom_404_url);
        if _404_path.exists() {
            let file = NamedFile::open(&_404_path).unwrap();
            let mut response = file.prefer_utf8(true);
            let mut buf = String::new();
            let _ = response.read_to_string(&mut buf);
            return HttpResponse::NotFound().body(buf);
        }
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
    enable_upload: bool,
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
        enable_upload,
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
    // 初始化日志
    env_logger::init_from_env(Env::default().default_filter_or("info"));
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
    let disable_powered_by = options.disable_powered_by;
    // 是否开启上传
    let enable_upload = options.upload.clone();
    // 获取文件路径
    let root_path = options.path.clone();
    // let log_path = options.log.clone();
    let security = options.security.clone();
    let mode = options.mode;
    // 要忽略的文件
    let ignore_pattern = Regex::new(&options.ignore_files).unwrap();
    let custom_404_url: String = if let Some(url) = &options.custom_404 {
        let mut _url = url.to_string();
        if _url.starts_with("/") {
            _url = _url.strip_prefix("/").unwrap().to_string();
        }
        if _url.ends_with("/") {
            _url = _url.strip_suffix("/").unwrap().to_string();
        }
        _url
    } else {
        String::from("")
    };

    // 代理地址支持环境变量
    // eg: http://${APP_URL} -> http://localhost:8080 with APP_URL=localhost:8080
    let proxy_regex = Regex::new(r"\$\{(.*?)\}").unwrap();

    // 是否反代所有路由
    let mut all_proxyed = false;

    // 反向代理
    let proxies: Vec<ProxyItem> = options
        .proxies
        .iter()
        .map(|item| {
            let s: Vec<&str> = item.split("->").collect();
            let target_url = proxy_regex.replace_all(s[1], |caps: &Captures | {
                let var_name = &caps[1];
                env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
            }).into_owned();
            let _proxy = ProxyItem {
                origin_path: s[0].to_string(),
                target_url: target_url,
            };
            if all_proxyed == false && _proxy.origin_path == "/" {
                all_proxyed = true;
            }
            info!("proxy: {} -> {}", _proxy.origin_path, _proxy.target_url);
            return _proxy;
        })
        .collect();

    // websocket 代理
    let ws_proxies: Vec<ProxyItem> = options
    .websocket_proxies
    .iter()
    .map(|item| {
        let s: Vec<&str> = item.split("->").collect();
        let target_url = proxy_regex.replace_all(s[1], |caps: &Captures | {
            let var_name = &caps[1];
            env::var(var_name).unwrap_or_else(|_| caps[0].to_string())
        }).into_owned();
        let _proxy = ProxyItem {
            origin_path: s[0].to_string(),
            target_url: target_url,
        };
        if all_proxyed == false && _proxy.origin_path == "/" {
            all_proxyed = true;
        }
        info!("websocket proxy: {} -> {}", _proxy.origin_path, _proxy.target_url);
        return _proxy;
    })
    .collect();
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
        let mut app = App::new()
            .wrap(middleware::Logger::default())
            .wrap(Condition::new(compress, middleware::Compress::default()))
            .wrap(Condition::new(
                !_user_id.is_empty(),
                HttpAuthentication::basic(basic_auth),
            ))
            // TODO 改成Condition::new，但是类型太复杂
            .wrap_fn(move |req: ServiceRequest, srv| {
                let fut = srv.call(req);
                async move {
                    let mut res = fut.await?;
                    if disable_powered_by {
                        return Ok(res);
                    }
                    let headers = res.headers_mut();
                    headers.append(
                        header::HeaderName::from_str("X-Powered-By").unwrap(),
                        header::HeaderValue::from_str(
                            format!("hs {}", env!("CARGO_PKG_VERSION")).as_str(),
                        )
                        .unwrap(),
                    );
                    Ok(res)
                }
            })
            // 使用actix-web的scope有问题，无法响应 /，只能响应子路由，所以手动处理
            // .service(handler)
            // .service(
            //     web::scope(if &base == "/" { "" } else { &base })
            .app_data(web::Data::new(AppState {
                base_url: base.clone(),
                username: _user_id.to_string(),
                password: _password.to_string(),
                root_path: PathBuf::from(&root_path),
                mode,
                cache,
                ignore_pattern: ignore_pattern.clone(),
                custom_404_url: custom_404_url.clone(),
                enable_upload,
            }));
        // 上传
        if enable_upload {
            app = app.service(web::scope(if &base == "/" { "" } else { &base }).service(upload))
        }
        // 反向代理
        for proxy in &proxies {
            let _origin_path = &proxy.origin_path;
            if _origin_path == "/" {
                app = app.app_data(web::Data::new(proxy.clone()))
                .app_data(web::Data::new(Client::new()))
                .default_service(web::to(forward_request))
            }
            app = app.service(
                web::scope(_origin_path)
                    .app_data(web::Data::new(proxy.clone()))
                    .app_data(web::Data::new(Client::new()))
                    .default_service(web::to(forward_request)),
            )
        }
        // websocket 代理
        for proxy in &ws_proxies {
            let _proxy = proxy.clone();
            app = app.service(
                web::scope(&_proxy.origin_path)
                    .app_data(web::Data::new(_proxy))
                    .default_service(web::to(ws_forward_request)),
            )
        }
        // 所有路由都被代理后就不需要文件路由了
        if !all_proxyed {
            app = app.service(handler);
        }
        return app;
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
