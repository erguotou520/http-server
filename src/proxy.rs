use actix_web::{
    error::{self},
    web, Error, HttpRequest, HttpResponse,
};
use awc::Client;
use url::Url;

#[derive(Clone)]
pub struct ProxyItem {
    pub origin_path: String,
    pub target_url: String,
}

// struct HttpClient {
//     // 单例模式
//     client: Client,
// }

// impl HttpClient {
//     pub fn new() -> Self {
//         HttpClient {
//             client: Client::new(),
//         }
//     }
// }

/// Forwards the incoming HTTP request using `awc`.
/// /api->http://example.com/ means /api/users -> http://example.com/users
/// /api->http://example.com/api means /api/users -> http://example.com/api/users
/// /api->http://example.com/app means /api/users -> http://example.com/app/api/users
/// /api->http://example.com/app/ means /api/users -> http://example.com/app/users
pub async fn forward_request(
    req: HttpRequest,
    payload: web::Payload,
    proxy_config: web::Data<ProxyItem>,
) -> Result<HttpResponse, Error> {
    let client = Client::new();
    // 从代理地址开始
    let mut new_url = Url::parse(&proxy_config.target_url).unwrap();
    // 去除代理url前缀
    let _left_path = req.uri().path().strip_prefix(&proxy_config.origin_path);
    if let Some(mut left_path) = _left_path {
        // 补全开头的/
        let left_path_str = if !left_path.starts_with("/") {
            format!("/{}", left_path)
        } else { left_path.to_string() };
        left_path = left_path_str.as_str();
        // 如果代理url是以/结尾的，那么就只追加left_path
        // 否则追加整个path
        let new_path = format!(
            "{}{}",
            &new_url.path(),
            if proxy_config.target_url.ends_with("/") {
                left_path
            } else {
                req.uri().path()
            }
        );
        new_url.set_path(&new_path);

        new_url.set_query(req.uri().query());

        let forwarded_req = client
            .request_from(new_url.as_str(), req.head())
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
    _proxy_config: web::Data<ProxyItem>,
) -> Result<HttpResponse, Error> {
    actix_ws_proxy::start(&req, format!("ws://127.0.0.1:5000"), payload).await
}