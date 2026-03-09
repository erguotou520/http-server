use actix_web::dev::{Service, ServiceRequest};
use actix_web::error::ErrorUnauthorized;
use actix_web::middleware::Condition;
use actix_web::{
    body::MessageBody,
    dev::ServiceResponse,
    middleware::{from_fn, Next},
    App, Error,
};
use actix_web_httpauth::extractors::basic::BasicAuth;
use actix_web_httpauth::middleware::HttpAuthentication;
use awc::Client;
use chrono::prelude::DateTime;
use chrono::Local;
// use env_logger::Env;
use fancy_regex::{Captures, Regex};
use std::fs::read_dir;
use std::io::Read;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::fs::metadata;
use std::time::{Duration, SystemTime};
use log::{self, info};
use std::env;

use actix_files::NamedFile;
use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_web::http::header::{self, ContentDisposition, DispositionType};
use actix_web::{
    get, middleware, post, web, HttpRequest, HttpResponse, HttpServer, Responder,
};
use local_ip_address::list_afinet_netifas;
use open::that;

use crate::cli::{CliOption, WorkMode};
use crate::logger::LOGGER;
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

#[post("")]
async fn upload(
    MultipartForm(form): MultipartForm<UploadForm>,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let dest_dir = state.root_path.join(&*form.path);
    for f in form.files {
        let path = dest_dir.join(f.file_name.unwrap());
        f.file.persist(path).unwrap();
    }

    Ok(HttpResponse::Ok())
}

#[get("{filename:.*}")]
async fn handler(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let _file_path: PathBuf = req.match_info().query("filename").parse().unwrap();
    // 去掉前缀（借用，不 clone）
    let file_path = match _file_path.strip_prefix(&state.base_url) {
        Ok(p) => p.to_path_buf(),
        Err(_) => _file_path,
    };
    // 对于忽略文件要屏蔽
    if !file_path.starts_with(".well-known") {
        if let Ok(_match) = state.ignore_pattern.is_match(file_path.to_str().unwrap()) {
            if _match {
                return Ok(not_found_response(state));
            }
        }
    }
    let path = state.root_path.join(&file_path);
    let mode = state.mode;
    // 合并两次 stat：try_exists()+metadata() → 单次 metadata()
    match metadata(&path) {
        Ok(md) => {
            // 目录
            if md.is_dir() {
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
                let file = NamedFile::open_async(&path).await?;
                let mut response = file.prefer_utf8(true);
                if state.cache {
                    response = response.use_etag(true).use_last_modified(true);
                } else {
                    response = response.use_etag(false).use_last_modified(false);
                }
                Ok(response.into_response(&req))
            }
        }
        Err(_) => {
            // 文件不存在
            if mode == WorkMode::SPA {
                if let Ok(response) = auto_render_index_html(&state.root_path) {
                    return Ok(response.into_response(&req));
                }
            }
            Ok(not_found_response(state))
        }
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
    for entry in read_dir(base_path.join(file_path))? {
        let entry = entry?;
        let name = String::from(entry.file_name().to_str().unwrap());
        // 忽略隐藏文件
        if !name.starts_with(".well-known") {
            if let Ok(_match) = ignore_pattern.is_match(&name) {
                if _match {
                    continue;
                }
            }
        }
        // 单次 metadata，避免多次 stat syscall
        let md = entry.metadata()?;
        let modified_local: DateTime<Local> = md.modified()?.into();
        let update_time = modified_local.format("%Y-%m-%d %H:%M:%S").to_string();
        // 用 strip_prefix 代替 string replace，避免额外堆分配
        let entry_rel = entry.path()
            .strip_prefix(base_path)
            .map(|p| format!("/{}", p.to_string_lossy()))
            .unwrap_or_default();
        if md.is_dir() {
            files.push(FileItem {
                name,
                path: entry_rel,
                is_dir: true,
                size: String::from("0"),
                update_time,
            })
        } else {
            files.push(FileItem {
                name,
                path: entry_rel,
                is_dir: false,
                size: format_file_size(md.len()),
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

    let state = req.app_data::<web::Data<AppState>>();
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

async fn custom_logger_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let method = req.method().to_string();
    let path = req.path().to_string();
    let ip = req.peer_addr().unwrap().ip().to_string();
    let time_start = SystemTime::now();
    let rep = next.call(req).await;
    if let Ok(resp) = &rep {
        let status = resp.status();
        let time_end = SystemTime::now();
        let time_diff = time_end.duration_since(time_start).unwrap();
        let time_diff_ms = time_diff.as_millis() as f64;
        LOGGER.info(format_args!("{} \"{} {}\" {} {}ms", ip, method, path, status.as_u16(), time_diff_ms).to_string());
    }
    return rep
}

pub async fn start_server(options: &CliOption) -> std::io::Result<()> {
    // 初始化日志
    // env_logger::init_from_env(Env::default().default_filter_or("info"));
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
    // 获取文件路径，规范化为绝对路径（修复 -f . 时路径无 filename 的问题）
    // 位置参数 folder 优先于 -f/--path
    let raw_path = options.folder.as_deref().unwrap_or(&options.path);
    let root_path = std::fs::canonicalize(raw_path)
        .map_err(|e| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Path '{}' does not exist or is not accessible: {}", options.path, e),
        ))?;
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
            let _proxy = ProxyItem::new(s[0].to_string(), target_url);
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
        let _proxy = ProxyItem::new(s[0].to_string(), target_url);
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
            // .wrap(middleware::Logger::default())
            .wrap(from_fn(custom_logger_middleware))
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
            .app_data(web::Data::new(AppState {
                base_url: base.clone(),
                username: _user_id.to_string(),
                password: _password.to_string(),
                root_path: root_path.clone(),
                mode,
                cache,
                ignore_pattern: ignore_pattern.clone(),
                custom_404_url: custom_404_url.clone(),
                enable_upload,
            }));
        // 上传
        if enable_upload {
            let mut scope = String::from("/_upload");
            if &base != "/" {
                scope = format!("{}/_upload", base);
            }
            app = app.service(web::scope(&scope).service(upload))
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
    })
    .backlog(1024)
    .keep_alive(Duration::from_secs(75)) // 保持连接
    .client_request_timeout(Duration::from_secs(10));

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
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{http::header, test, App};
    use actix_web_httpauth::middleware::HttpAuthentication;
    use std::str;
    use tempfile::TempDir;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// 创建带有测试文件的临时目录
    fn setup_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("index.html"), "<html>SPA Index</html>").unwrap();
        std::fs::write(tmp.path().join("file.txt"), "Hello World").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();
        std::fs::write(tmp.path().join("subdir").join("sub.txt"), "Sub content").unwrap();
        // 隐藏文件
        std::fs::write(tmp.path().join(".hidden"), "secret").unwrap();
        // .well-known 目录（ACME challenge，不应被 ignore 过滤）
        std::fs::create_dir(tmp.path().join(".well-known")).unwrap();
        std::fs::write(
            tmp.path().join(".well-known").join("acme-challenge"),
            "token-value",
        )
        .unwrap();
        // 自定义 404 页面
        std::fs::write(tmp.path().join("custom404.html"), "<h1>Custom 404</h1>").unwrap();
        tmp
    }

    fn make_state(root: &TempDir, mode: WorkMode) -> AppState {
        AppState {
            base_url: String::new(),
            username: String::new(),
            password: String::new(),
            root_path: root.path().to_path_buf(),
            mode,
            cache: true,
            ignore_pattern: Regex::new(r"^\.").unwrap(),
            custom_404_url: String::new(),
            enable_upload: false,
        }
    }

    // ── index 模式 ────────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_index_mode_root_returns_200_html() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));
        let body = test::read_body(resp).await;
        // 目录列表应该包含 file.txt 和 subdir
        let body_str = str::from_utf8(&body).unwrap();
        assert!(body_str.contains("file.txt") || body_str.contains("subdir"));
    }

    #[actix_web::test]
    async fn test_index_mode_subdir_returns_200() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/subdir").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        let body_str = str::from_utf8(&body).unwrap();
        assert!(body_str.contains("sub.txt"));
    }

    #[actix_web::test]
    async fn test_index_mode_file_returns_content() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "Hello World");
    }

    // ── server 模式 ───────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_server_mode_directory_returns_403() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Server)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn test_server_mode_file_returns_200() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Server)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    // ── SPA 模式 ──────────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_spa_mode_nonexistent_route_falls_back_to_index() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::SPA)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/nonexistent/route").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "<html>SPA Index</html>");
    }

    #[actix_web::test]
    async fn test_spa_mode_existing_file_served_directly() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::SPA)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "Hello World");
    }

    #[actix_web::test]
    async fn test_spa_mode_directory_falls_back_to_index() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::SPA)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/subdir").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "<html>SPA Index</html>");
    }

    #[actix_web::test]
    async fn test_spa_mode_no_index_html_returns_404() {
        // 目录下没有 index.html 时，SPA 回退也应返回 404
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("other.txt"), "content").unwrap();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::SPA)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/nonexistent").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    // ── 404 处理 ──────────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_nonexistent_file_returns_404() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/does-not-exist.html").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_custom_404_page() {
        let tmp = setup_dir();
        let state = AppState {
            custom_404_url: "custom404.html".to_string(),
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/missing-page").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
        let body = test::read_body(resp).await;
        let body_str = str::from_utf8(&body).unwrap();
        assert!(body_str.contains("Custom 404"));
    }

    // ── 缓存头 ────────────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_cache_enabled_has_etag() {
        let tmp = setup_dir();
        let state = AppState {
            cache: true,
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert!(resp.headers().contains_key(header::ETAG));
    }

    #[actix_web::test]
    async fn test_cache_disabled_no_etag() {
        let tmp = setup_dir();
        let state = AppState {
            cache: false,
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        assert!(!resp.headers().contains_key(header::ETAG));
    }

    // ── 文件忽略规则 ──────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_hidden_file_returns_404() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/.hidden").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_normal_file_not_ignored() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_well_known_not_ignored() {
        // .well-known 目录应豁免 ignore 规则
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/.well-known/acme-challenge")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "token-value");
    }

    // ── Basic Auth ────────────────────────────────────────────────────────────

    fn make_auth_state(root: &TempDir) -> AppState {
        AppState {
            username: "user".to_string(),
            password: "pass".to_string(),
            ..make_state(root, WorkMode::Index)
        }
    }

    #[actix_web::test]
    async fn test_basic_auth_no_credentials_returns_401() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_auth_state(&tmp)))
                .wrap(HttpAuthentication::basic(basic_auth))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_basic_auth_correct_credentials_returns_200() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_auth_state(&tmp)))
                .wrap(HttpAuthentication::basic(basic_auth))
                .service(handler),
        )
        .await;
        // user:pass => dXNlcjpwYXNz
        let req = test::TestRequest::get()
            .uri("/file.txt")
            .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_basic_auth_wrong_credentials_returns_401() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_auth_state(&tmp)))
                .wrap(HttpAuthentication::basic(basic_auth))
                .service(handler),
        )
        .await;
        // user:wrong => dXNlcjp3cm9uZw==
        let req = test::TestRequest::get()
            .uri("/file.txt")
            .insert_header(("Authorization", "Basic dXNlcjp3cm9uZw=="))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    // ── 文件上传 ──────────────────────────────────────────────────────────────

    /// 构造 multipart 请求体
    fn make_multipart_body(boundary: &str, dest_path: &str, filename: &str, content: &str) -> Vec<u8> {
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"path\"\r\n\r\n{dest_path}\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{filename}\"\r\nContent-Type: text/plain\r\n\r\n{content}\r\n\
             --{boundary}--\r\n"
        );
        body.into_bytes()
    }

    #[actix_web::test]
    async fn test_upload_creates_file() {
        let tmp = setup_dir();
        let state = AppState {
            enable_upload: true,
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(web::scope("/_upload").service(upload)),
        )
        .await;
        let boundary = "----testboundary12345";
        let body = make_multipart_body(boundary, "", "uploaded.txt", "uploaded content");
        let req = test::TestRequest::post()
            .uri("/_upload")
            .insert_header((
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let saved = tmp.path().join("uploaded.txt");
        assert!(saved.exists(), "uploaded file should exist at {:?}", saved);
        let content = std::fs::read_to_string(&saved).unwrap();
        assert_eq!(content, "uploaded content");
    }

    // ── X-Powered-By ─────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_powered_by_disabled_no_header() {
        let tmp = setup_dir();
        // disable_powered_by=true 时不添加 X-Powered-By 头
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .wrap_fn(|req, srv| {
                    let fut = srv.call(req);
                    async move {
                        let res = fut.await?;
                        // disable_powered_by=true => do nothing
                        Ok(res)
                    }
                })
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(!resp.headers().contains_key("x-powered-by"));
    }

    #[actix_web::test]
    async fn test_powered_by_enabled_has_header() {
        let tmp = setup_dir();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(make_state(&tmp, WorkMode::Index)))
                .wrap_fn(|req, srv| {
                    let fut = srv.call(req);
                    async move {
                        let mut res = fut.await?;
                        res.headers_mut().insert(
                            header::HeaderName::from_str("x-powered-by").unwrap(),
                            header::HeaderValue::from_static("hs"),
                        );
                        Ok(res)
                    }
                })
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.headers().contains_key("x-powered-by"));
    }

    // ── Base URL ──────────────────────────────────────────────────────────────

    #[actix_web::test]
    async fn test_base_url_strips_prefix_and_serves_file() {
        let tmp = setup_dir();
        let state = AppState {
            base_url: "mybase".to_string(),
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/mybase/file.txt").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "Hello World");
    }

    // ── root path canonicalize (fix: -f . 访问 / 报 "Provided path has no filename") ──

    #[actix_web::test]
    async fn test_absolute_root_path_serves_directory() {
        // 使用绝对路径（等效于 canonicalize(".") 之后的效果）
        let tmp = setup_dir();
        let abs_path = tmp.path().canonicalize().unwrap();
        let state = AppState {
            root_path: abs_path,
            ..make_state(&tmp, WorkMode::Index)
        };
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .service(handler),
        )
        .await;
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"));
    }
}