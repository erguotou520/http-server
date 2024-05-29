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
    pub target_url: String,
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
    let mut new_url = Url::parse(&proxy_config.target_url).unwrap();
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
