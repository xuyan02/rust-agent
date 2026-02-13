use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Bytes,
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let inner = reqwest::Client::builder()
            .user_agent("agent/0.1")
            .build()
            .with_context(|| "failed to build http client")?;
        Ok(Self { inner })
    }

    pub async fn send(&self, req: HttpRequest) -> Result<HttpResponse> {
        let mut headers = HeaderMap::new();
        for (k, v) in req.headers {
            let name = HeaderName::from_bytes(k.as_bytes())
                .with_context(|| format!("invalid header name: {k}"))?;
            let value = HeaderValue::from_str(&v)
                .with_context(|| format!("invalid header value for {k}"))?;
            headers.insert(name, value);
        }

        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .with_context(|| format!("invalid http method: {}", req.method))?;

        let resp = self
            .inner
            .request(method, req.url)
            .headers(headers)
            .body(req.body)
            .send()
            .await
            .with_context(|| "http request failed")?;

        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .with_context(|| "failed to read response body")?;

        Ok(HttpResponse { status, body })
    }
}
