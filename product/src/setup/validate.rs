use super::Error;
use crate::config::Auth;

pub fn api_base(raw: &str) -> Result<url::Url, Error> {
    let raw_query = raw
        .split_once('?')
        .map(|(_, query)| query.split_once('#').map_or(query, |(query, _)| query));
    let url = url::Url::parse(raw)
        .map_err(|_| Error::message("upstream API base must be a valid absolute URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::message("upstream API base must use http or https"));
    }
    if url.host_str().is_none() {
        return Err(Error::message(
            "upstream API base absolute URL must include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::message(
            "upstream API base must not contain credentials",
        ));
    }
    if url.fragment().is_some() {
        return Err(Error::message(
            "upstream API base must not contain a fragment",
        ));
    }
    if raw_query == Some("") || url.query() != raw_query {
        return Err(Error::message(
            "upstream API base query must be nonempty and already losslessly encoded",
        ));
    }
    Ok(url)
}

pub fn proxy(raw: &str) -> Result<url::Url, Error> {
    let url =
        url::Url::parse(raw).map_err(|_| Error::message("proxy must be a valid absolute URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::message(
            "proxy must be an http(s) URL with a host and no credentials, query, or fragment",
        ));
    }
    Ok(url)
}

/// Check current-process availability only. This never reads or returns the secret value.
pub fn available_auth(auth: &Auth) -> Result<(), Error> {
    if let Auth::Env { name, .. } = auth {
        if std::env::var_os(name).is_none() {
            return Err(Error::message(format!(
                "environment variable {name} is unavailable in the current process; it must contain the complete header value"
            )));
        }
    }
    Ok(())
}

pub(crate) fn auth_header(auth: &Auth) -> Result<Option<(String, String)>, Error> {
    match auth {
        Auth::None => Ok(None),
        Auth::Forward => Err(Error::message(
            "forward auth cannot be checked without an incoming client credential",
        )),
        Auth::Env { header, name } => {
            let value = std::env::var(name).map_err(|_| {
                Error::message(format!(
                    "environment variable {name} is unavailable; provide the complete header value"
                ))
            })?;
            Ok(Some((header.clone(), value)))
        }
    }
}
