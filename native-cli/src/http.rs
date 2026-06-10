use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, USER_AGENT};
use std::time::Duration;

const USER_AGENT_VALUE: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct HttpClient {
    client: Client,
    timeout: Duration,
}

impl HttpClient {
    pub fn new(timeout: Duration) -> Result<Self> {
        Self::build(timeout, false)
    }

    pub fn new_insecure_localhost(timeout: Duration) -> Result<Self> {
        Self::build(timeout, true)
    }

    fn build(timeout: Duration, accept_invalid_certs: bool) -> Result<Self> {
        let mut builder = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::limited(4));
        if accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let client = builder.build().context("build HTTP client")?;
        Ok(Self { client, timeout })
    }

    pub fn fetch_text(&self, url: &str, headers: &HeaderMap) -> Result<String> {
        self.client
            .get(url)
            .headers(headers.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("GET {url} status"))?
            .text()
            .with_context(|| format!("read body from {url}"))
    }

    pub fn fetch_json_value(&self, url: &str, headers: &HeaderMap) -> Result<serde_json::Value> {
        let text = self.fetch_text(url, headers)?;
        Ok(serde_json::from_str(&text).with_context(|| format!("parse JSON from {url}"))?)
    }

    pub fn post_text(&self, url: &str, headers: &HeaderMap, body: &[u8]) -> Result<String> {
        self.client
            .post(url)
            .headers(headers.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .body(body.to_vec())
            .send()
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("POST {url} status"))?
            .text()
            .with_context(|| format!("read body from {url}"))
    }

    pub fn post_connect_json(
        &self,
        url: &str,
        csrf_token: &str,
        body: &serde_json::Value,
    ) -> Result<String> {
        let payload = serde_json::to_vec(body).context("encode connect JSON body")?;
        self.client
            .post(url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header("Content-Type", "application/json")
            .header("Content-Length", payload.len().to_string())
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", csrf_token)
            .body(payload)
            .send()
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("POST {url} status"))?
            .text()
            .with_context(|| format!("read body from {url}"))
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

pub fn cookie_header(value: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(value).context("invalid cookie header")?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    Ok(headers)
}

pub fn html_headers(cookie: &str) -> Result<HeaderMap> {
    let mut headers = cookie_header(cookie)?;
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    Ok(headers)
}
