use actix_web::{
    error::{self},
    web, Error, HttpRequest, HttpResponse,
};
use awc::Client;
use url::Url;

use crate::ws_proxy;

#[derive(Clone)]
pub struct ProxyItem {
    pub origin_path: String,
    /// 原始字符串，仅用于显示/日志
    pub target_url: String,
    /// 启动时预解析，避免每次请求重复 parse
    pub target_url_parsed: Url,
}

impl ProxyItem {
    pub fn new(origin_path: String, target_url: String) -> Self {
        let target_url_parsed = Url::parse(&target_url)
            .unwrap_or_else(|e| panic!("Invalid proxy target URL '{}': {}", target_url, e));
        Self { origin_path, target_url, target_url_parsed }
    }
}

/// Forwards the incoming HTTP request using `awc`.
/// /api->http://example.com/ means /api/users -> http://example.com/users
/// /api->http://example.com/api means /api/users -> http://example.com/api/users
/// /api->http://example.com/app means /api/users -> http://example.com/app/api/users
/// /api->http://example.com/app/ means /api/users -> http://example.com/app/users
pub async fn forward_request(
    req: HttpRequest,
    payload: web::Payload,
    proxy_config: web::Data<ProxyItem>,
    client: web::Data<Client>,
) -> Result<HttpResponse, Error> {
    if let Ok(proxy_url) = get_proxy_path(&req, &proxy_config) {
        let forwarded_req = client
            .request_from(proxy_url.as_str(), req.head())
            .no_decompress();

        // TODO: This forwarded implementation is incomplete as it only handles the unofficial
        // X-Forwarded-For header but not the official Forwarded one.
        //   let forwarded_req = match peer_addr {
        //       Some(PeerAddr(addr)) => {
        //           forwarded_req.insert_header(("x-forwarded-for", addr.ip().to_string()))
        //       }
        //       None => forwarded_req,
        //   };

        let res = forwarded_req
            .send_stream(payload)
            .await
            .map_err(error::ErrorInternalServerError)?;

        let mut client_resp = HttpResponse::build(res.status());
        // Remove `Connection` as per
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Connection#Directives
        for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection")
        {
            client_resp.insert_header((header_name.clone(), header_value.clone()));
        }

        Ok(client_resp.streaming(res))
    } else {
        Ok(HttpResponse::InternalServerError().body("Invalid proxy configuration"))
    }
}

pub async fn ws_forward_request(
    req: HttpRequest,
    payload: web::Payload,
    proxy_config: web::Data<ProxyItem>,
) -> Result<HttpResponse, Error> {
    if let Ok(proxy_url) = get_proxy_path(&req, &proxy_config) {
        ws_proxy::start(&req, proxy_url.to_string(), payload).await
    } else {
        Ok(HttpResponse::InternalServerError().body("Invalid websocket proxy configuration"))
    }
}

fn get_proxy_path(req: &HttpRequest, proxy_config: &ProxyItem) -> Result<Url, bool> {
    // 直接 clone 预解析好的 Url，避免每次请求重新 parse 字符串
    let mut new_url = proxy_config.target_url_parsed.clone();
    // 去除代理url前缀
    let _left_path = req.uri().path().strip_prefix(&proxy_config.origin_path);
    if let Some(mut left_path) = _left_path {
        // 补全开头的/
        let left_path_str = if !left_path.starts_with("/") {
            format!("/{}", left_path)
        } else {
            left_path.to_string()
        };
        left_path = left_path_str.as_str();
        // 如果代理url是以/结尾的，那么就只追加left_path
        // 否则追加整个path
        let joined_path = new_url.join(if proxy_config.target_url.ends_with("/") {
            left_path
        } else {
            req.uri().path()
        });

        if let Ok(joined_url) = joined_path {
            new_url = joined_url;
        }

        new_url.set_query(req.uri().query());
        Ok(new_url)
    } else {
        Err(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn make_proxy(origin: &str, target: &str) -> ProxyItem {
        ProxyItem::new(origin.to_string(), target.to_string())
    }

    /// /api -> http://example.com/  =>  /api/users  ->  http://example.com/users
    #[test]
    fn test_proxy_path_trailing_slash_target() {
        let req = TestRequest::get()
            .uri("/api/users")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/");
        let url = get_proxy_path(&req, &proxy).unwrap();
        assert_eq!(url.as_str(), "http://example.com/users");
    }

    /// /api -> http://example.com/api  =>  /api/users  ->  http://example.com/api/users
    #[test]
    fn test_proxy_path_no_trailing_slash_target() {
        let req = TestRequest::get()
            .uri("/api/users")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/api");
        let url = get_proxy_path(&req, &proxy).unwrap();
        assert_eq!(url.as_str(), "http://example.com/api/users");
    }

    /// /api -> http://example.com/app/  =>  /api/users  ->  http://example.com/users
    /// (left_path="/users", url::Url::join with absolute path replaces all path segments)
    #[test]
    fn test_proxy_path_sub_path_trailing_slash() {
        let req = TestRequest::get()
            .uri("/api/users")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/app/");
        let url = get_proxy_path(&req, &proxy).unwrap();
        assert_eq!(url.as_str(), "http://example.com/users");
    }

    /// query string should be preserved
    #[test]
    fn test_proxy_path_preserves_query() {
        let req = TestRequest::get()
            .uri("/api/search?q=hello&page=1")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/");
        let url = get_proxy_path(&req, &proxy).unwrap();
        assert_eq!(url.path(), "/search");
        assert_eq!(url.query(), Some("q=hello&page=1"));
    }

    /// path prefix not matching returns Err
    #[test]
    fn test_proxy_path_no_prefix_match() {
        let req = TestRequest::get()
            .uri("/other/path")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/");
        assert!(get_proxy_path(&req, &proxy).is_err());
    }

    /// exact match on prefix root  =>  /api  ->  http://example.com/
    #[test]
    fn test_proxy_path_exact_prefix() {
        let req = TestRequest::get()
            .uri("/api")
            .to_http_request();
        let proxy = make_proxy("/api", "http://example.com/");
        let url = get_proxy_path(&req, &proxy).unwrap();
        assert_eq!(url.path(), "/");
    }
}
