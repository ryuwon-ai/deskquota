use std::{fmt, fs, io, time::Duration};

use axum::http::{HeaderMap, Method};
use reqwest::{Certificate, NoProxy, Proxy};

use super::body_budget::BudgetedBody;
use crate::config::Upstream;

#[derive(Debug)]
pub enum BuildError {
    ReadCa(io::Error),
    InvalidCa(reqwest::Error),
    EmptyCa,
    Client(reqwest::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadCa(_) => formatter.write_str("failed to read configured CA bundle"),
            Self::InvalidCa(_) => {
                formatter.write_str("configured CA bundle is not valid PEM certificates")
            }
            Self::EmptyCa => formatter.write_str("configured CA bundle contains no certificates"),
            Self::Client(_) => formatter.write_str("failed to build upstream HTTP client"),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadCa(error) => Some(error),
            Self::InvalidCa(error) | Self::Client(error) => Some(error),
            Self::EmptyCa => None,
        }
    }
}

#[derive(Clone)]
pub struct UpstreamClient {
    inner: reqwest::Client,
}

impl UpstreamClient {
    pub fn new(timeout: Duration, upstream: &Upstream) -> Result<Self, BuildError> {
        Ok(Self {
            inner: build_client(timeout, upstream)?,
        })
    }
}

pub(crate) fn build_client(
    timeout: Duration,
    upstream: &Upstream,
) -> Result<reqwest::Client, BuildError> {
    let mut builder = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .pool_max_idle_per_host(16);
    if let Some(proxy) = &upstream.proxy {
        let bypass = NoProxy::from_string("localhost,127.0.0.0/8,::1");
        let proxy = Proxy::all(proxy.as_str())
            .map_err(BuildError::Client)?
            .no_proxy(bypass);
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }
    if let Some(path) = &upstream.ca_bundle {
        let pem = fs::read(path).map_err(BuildError::ReadCa)?;
        let certificates = Certificate::from_pem_bundle(&pem).map_err(BuildError::InvalidCa)?;
        if certificates.is_empty() {
            return Err(BuildError::EmptyCa);
        }
        builder = builder.tls_certs_merge(certificates);
    }
    builder.build().map_err(BuildError::Client)
}

impl UpstreamClient {
    pub async fn send(
        &self,
        method: Method,
        url: url::Url,
        headers: HeaderMap,
        body: BudgetedBody,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.inner
            .request(method, url)
            .headers(headers)
            .body(body.into_bytes())
            .send()
            .await
    }
}
