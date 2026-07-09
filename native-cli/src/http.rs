use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, USER_AGENT};
use std::time::Duration;

const USER_AGENT_VALUE: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct HttpClient {
    client: Client,
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
        Ok(Self { client })
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

    /// POST raw bytes and return the response body as bytes (no UTF-8 assumption).
    /// Used for gRPC-web protobuf frames that are not valid text.
    pub fn post_bytes(&self, url: &str, headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>> {
        let response = self
            .client
            .post(url)
            .headers(headers.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .body(body.to_vec())
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .with_context(|| format!("read body from {url}"))?
            .to_vec();
        if !status.is_success() {
            anyhow::bail!("POST {url} status {status}");
        }
        Ok(bytes)
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

    /// POST a form-encoded body and return the response text.
    /// Used for OAuth token refresh exchanges.
    pub fn post_form(&self, url: &str, body: &str) -> Result<String> {
        self.client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header(USER_AGENT, USER_AGENT_VALUE)
            .body(body.to_string())
            .send()
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("POST {url} status"))?
            .text()
            .with_context(|| format!("read body from {url}"))
    }

    /// POST a JSON body with a Bearer token and custom User-Agent, returning
    /// the HTTP status code and response text without asserting success. This
    /// lets callers branch on 401/403/etc. for the Cloud Code API.
    pub fn post_bearer_text(
        &self,
        url: &str,
        bearer: &str,
        user_agent: &str,
        body: &serde_json::Value,
    ) -> Result<(u16, String)> {
        let payload = serde_json::to_vec(body).context("encode JSON body")?;
        let response = self
            .client
            .post(url)
            .header(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {bearer}"))
                    .context("invalid bearer token header")?,
            )
            .header("Content-Type", "application/json")
            .header(USER_AGENT, HeaderValue::from_str(user_agent).unwrap_or_else(|_| HeaderValue::from_static("antigravity")))
            .body(payload)
            .send()
            .with_context(|| format!("POST {url}"))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .with_context(|| format!("read body from {url}"))?;
        Ok((status, text))
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
