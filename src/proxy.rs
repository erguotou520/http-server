use actix_web::{error, web, Error, HttpRequest, HttpResponse};
use awc::Client;
use url::Url;

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
pub async fn forward_request(
  req: HttpRequest,
  payload: web::Payload,
  target_url: web::Data<String>,
  url: web::Data<Url>,
) -> Result<HttpResponse, Error> {
  let client = Client::new();
  let mut new_url = (**url).clone();
  new_url.set_path(req.uri().path());
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
  for (header_name, header_value) in res.headers().iter().filter(|(h, _)| *h != "connection") {
      client_resp.insert_header((header_name.clone(), header_value.clone()));
  }

  Ok(client_resp.streaming(res))
}