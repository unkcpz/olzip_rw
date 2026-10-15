use bytes::Bytes;

use crate::Error;

/// What the rest of the code needs from an HTTP response.
pub(crate) struct HttpResponse {
    pub content_length: Option<String>,
    pub content_range: Option<String>,
    pub accept_ranges: bool,
    pub body: Bytes, // empty for HEAD
}

// ---------------------------------------------------------------- native
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct HttpClient {
    client: reqwest::blocking::Client,
}

#[cfg(not(target_arch = "wasm32"))]
impl HttpClient {
    pub fn new(host: String) -> Self {
        let retry_options = reqwest::retry::for_host(host)
            .max_retries_per_request(3)
            .max_extra_load(0.25)
            .classify_fn(|req_rep| {
                if req_rep.status() == Some(http::StatusCode::TOO_MANY_REQUESTS) {
                    req_rep.retryable()
                } else {
                    req_rep.success()
                }
            });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .retry(retry_options)
            .build()
            .unwrap();
        HttpClient { client }
    }

    fn header(resp: &reqwest::blocking::Response, name: &str) -> Option<String> {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    }

    pub fn head(&self, uri: &str) -> Result<HttpResponse, Error> {
        let resp = self
            .client
            .head(uri)
            .send()
            .map_err(|e| Error::HttpHead(e.to_string()))?;
        Ok(HttpResponse {
            content_length: Self::header(&resp, "content-length"),
            content_range: None,
            accept_ranges: resp.headers().contains_key("accept-ranges"),
            body: Bytes::new(),
        })
    }

    /// `end` is exclusive.
    pub fn get_range(&self, uri: &str, start: u64, end: u64) -> Result<HttpResponse, Error> {
        let resp = self
            .client
            .get(uri)
            .header(reqwest::header::RANGE, format!("bytes={}-{}", start, end - 1))
            .send()
            .map_err(|e| Error::HttpRangeGet(e.to_string()))?;
        let content_length = Self::header(&resp, "content-length");
        let content_range = Self::header(&resp, "content-range");
        let accept_ranges = resp.headers().contains_key("accept-ranges");
        let body = resp
            .bytes()
            .map_err(|e| Error::HttpRangeGet(e.to_string()))?;
        Ok(HttpResponse {
            content_length,
            content_range,
            accept_ranges,
            body,
        })
    }
}

// ---------------------------------------------------------------- wasm
#[cfg(target_arch = "wasm32")]
pub(crate) struct HttpClient;

#[cfg(target_arch = "wasm32")]
impl HttpClient {
    pub fn new(_host: String) -> Self {
        HttpClient
    }

    pub fn head(&self, uri: &str) -> Result<HttpResponse, Error> {
        let run = || -> Result<HttpResponse, wasm_bindgen::JsValue> {
            let xhr = web_sys::XmlHttpRequest::new()?;
            xhr.open_with_async("HEAD", uri, false)?; // false = synchronous (worker only!)
            xhr.send()?;
            let status = xhr.status()?;
            if status != 200 {
                return Err(wasm_bindgen::JsValue::from_str(&format!("HTTP {status}")));
            }
            Ok(HttpResponse {
                content_length: xhr.get_response_header("Content-Length")?,
                content_range: None,
                accept_ranges: xhr.get_response_header("Accept-Ranges")?.is_some(),
                body: Bytes::new(),
            })
        };
        run().map_err(|e| Error::HttpHead(format!("{e:?}")))
    }

    /// `end` is exclusive.
    pub fn get_range(&self, uri: &str, start: u64, end: u64) -> Result<HttpResponse, Error> {
        let run = || -> Result<HttpResponse, wasm_bindgen::JsValue> {
            let xhr = web_sys::XmlHttpRequest::new()?;
            xhr.open_with_async("GET", uri, false)?;
            xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
            xhr.set_request_header("Range", &format!("bytes={}-{}", start, end - 1))?;
            xhr.send()?;
            let status = xhr.status()?;
            if status != 206 {
                return Err(wasm_bindgen::JsValue::from_str(&format!(
                    "expected HTTP 206, got {status}"
                )));
            }
            let body = js_sys::Uint8Array::new(&xhr.response()?).to_vec();
            Ok(HttpResponse {
                content_length: xhr.get_response_header("Content-Length")?,
                content_range: xhr.get_response_header("Content-Range")?,
                accept_ranges: xhr.get_response_header("Accept-Ranges")?.is_some(),
                body: Bytes::from(body),
            })
        };
        run().map_err(|e| Error::HttpRangeGet(format!("{e:?}")))
    }
}