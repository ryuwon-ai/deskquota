use std::collections::HashSet;
use std::net::SocketAddr;
use std::num::NonZeroU64;

use super::{
    Accounting, Auth, AuthInput, CancelPolicy, Config, ConfigError, ConfigInput, Endpoint, Limit,
    LimitInput, MAX_CONCURRENCY, MIN_CONCURRENCY, Model, ModelInput, Quota, Root, RootInput,
    Upstream,
};

const DEFAULT_LISTEN: &str = "127.0.0.1:4141";
const DEFAULT_CONCURRENCY: u8 = 1;
const MAX_ROOTS: usize = 16;
const MAX_ROOT_ID_BYTES: usize = 64;

pub(super) fn validate(input: ConfigInput) -> Result<Config, ConfigError> {
    let listen = validate_listen(input.listen.as_deref().unwrap_or(DEFAULT_LISTEN))?;
    let concurrency = input.concurrency.unwrap_or(DEFAULT_CONCURRENCY);
    if !(MIN_CONCURRENCY..=MAX_CONCURRENCY).contains(&concurrency) {
        return invalid("concurrency must be between 1 and 16");
    }

    let startup_hold_secs = input.startup_hold_secs.unwrap_or(60);
    if startup_hold_secs > 3600 {
        return invalid("startup_hold_secs must be between 0 and 3600");
    }
    if input.cache.is_some_and(|cache| !cache.is_valid()) {
        return invalid("cache ttl_secs must be 1..3600 and max_history must be 1..64");
    }
    let cancel_policy = validate_cancel_policy(input.cancel_policy.as_deref().unwrap_or("drain"))?;
    let accounting = validate_accounting(input.accounting.as_deref().unwrap_or("actual"))?;
    let upstream = validate_upstream(
        input.upstream.api_base,
        input.upstream.auth,
        input.upstream.proxy,
        input.upstream.ca_bundle,
    )?;
    let quota = Quota {
        rpm: validate_limit("quota.rpm", input.quota.rpm)?,
        tpm: validate_limit("quota.tpm", input.quota.tpm)?,
    };
    let models = validate_models(input.models)?;
    let roots = validate_roots(input.roots, &models)?;

    Ok(Config {
        listen,
        concurrency,
        startup_hold_secs,
        cache: input.cache,
        cancel_policy,
        accounting,
        retry_transient_429: input.retry_transient_429.unwrap_or(false),
        upstream,
        quota,
        models,
        roots,
    })
}

fn validate_cancel_policy(value: &str) -> Result<CancelPolicy, ConfigError> {
    match value {
        "drain" => Ok(CancelPolicy::Drain),
        "close" => Ok(CancelPolicy::Close),
        _ => invalid("cancel_policy must be drain or close"),
    }
}

fn validate_listen(value: &str) -> Result<SocketAddr, ConfigError> {
    let address: SocketAddr = value.parse().map_err(|_| {
        ConfigError::Validation("listen must be a numeric IP address and port".to_owned())
    })?;
    if !address.ip().is_loopback() {
        return invalid("listen must use a loopback IP address");
    }
    Ok(address)
}

fn validate_accounting(value: &str) -> Result<Accounting, ConfigError> {
    match value {
        "reserved" => Ok(Accounting::Reserved),
        "actual" => Ok(Accounting::Actual),
        _ => invalid("accounting must be reserved or actual"),
    }
}

fn validate_upstream(
    api_base: String,
    auth: AuthInput,
    proxy: Option<String>,
    ca_bundle: Option<std::path::PathBuf>,
) -> Result<Upstream, ConfigError> {
    let raw_query = raw_query_component(&api_base);
    let api_base = url::Url::parse(&api_base).map_err(|_| {
        ConfigError::Validation("upstream.api_base must be a valid absolute URL".to_owned())
    })?;
    if !matches!(api_base.scheme(), "http" | "https") {
        return invalid("upstream.api_base must use http or https");
    }
    if api_base.host_str().is_none() {
        return invalid("upstream.api_base URL must include a host");
    }
    if !api_base.username().is_empty() || api_base.password().is_some() {
        return invalid("upstream.api_base must not contain credentials or userinfo");
    }
    if api_base.fragment().is_some() {
        return invalid("upstream.api_base must not contain a fragment");
    }
    if raw_query == Some("") || api_base.query() != raw_query {
        return invalid(
            "upstream.api_base query must be nonempty and already use lossless URL encoding",
        );
    }

    Ok(Upstream {
        api_base,
        auth: validate_auth(auth)?,
        proxy: proxy.map(validate_proxy).transpose()?,
        ca_bundle,
    })
}

fn validate_proxy(raw: String) -> Result<url::Url, ConfigError> {
    let url = url::Url::parse(&raw).map_err(|_| {
        ConfigError::Validation("upstream.proxy must be a valid absolute URL".into())
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return invalid(
            "upstream.proxy must be an http(s) URL with a host and no credentials, query, or fragment",
        );
    }
    Ok(url)
}

fn raw_query_component(value: &str) -> Option<&str> {
    value
        .split_once('?')
        .map(|(_, query)| query.split_once('#').map_or(query, |(query, _)| query))
}

fn validate_auth(input: AuthInput) -> Result<Auth, ConfigError> {
    match input.mode.as_str() {
        "forward" => {
            reject_auth_fields("forward", &input)?;
            Ok(Auth::Forward)
        }
        "none" => {
            reject_auth_fields("none", &input)?;
            Ok(Auth::None)
        }
        "env" => {
            let header = required_nonempty("upstream.auth.header", input.header)?;
            if !is_http_token(&header) {
                return invalid("upstream.auth.header must be a valid HTTP header name");
            }
            if is_transport_reserved_header(&header) {
                return invalid("upstream.auth.header is reserved by HTTP transport");
            }
            let name = required_nonempty("upstream.auth.name", input.name)?;
            if !is_env_name(&name) {
                return invalid("upstream.auth.name must be a valid environment variable name");
            }
            Ok(Auth::Env { header, name })
        }
        _ => invalid("upstream.auth.mode must be forward, env, or none"),
    }
}

fn reject_auth_fields(mode: &str, input: &AuthInput) -> Result<(), ConfigError> {
    if input.header.is_some() || input.name.is_some() {
        return invalid(format!(
            "upstream.auth mode {mode:?} must not define header or name"
        ));
    }
    Ok(())
}

fn validate_limit(field: &str, input: LimitInput) -> Result<Limit, ConfigError> {
    match input.kind.as_str() {
        "known" => {
            let value = input.value.ok_or_else(|| {
                ConfigError::Validation(format!("{field} kind known requires value"))
            })?;
            let value = NonZeroU64::new(value).ok_or_else(|| {
                ConfigError::Validation(format!("{field} known value must be nonzero"))
            })?;
            Ok(Limit::Known(value))
        }
        "unknown" => {
            reject_limit_value(field, "unknown", input.value)?;
            Ok(Limit::Unknown)
        }
        "unlimited" => {
            reject_limit_value(field, "unlimited", input.value)?;
            Ok(Limit::Unlimited)
        }
        _ => invalid(format!("{field}.kind must be known, unknown, or unlimited")),
    }
}

fn reject_limit_value(field: &str, kind: &str, value: Option<u64>) -> Result<(), ConfigError> {
    if value.is_some() {
        return invalid(format!("{field} kind {kind} must not define value"));
    }
    Ok(())
}

fn validate_models(inputs: Vec<ModelInput>) -> Result<Vec<Model>, ConfigError> {
    let mut ids = HashSet::with_capacity(inputs.len());
    let mut models = Vec::with_capacity(inputs.len());
    for input in inputs {
        if input.id.is_empty() {
            return invalid("model id must not be empty");
        }
        if !ids.insert(input.id.clone()) {
            return invalid("model ids must be unique");
        }
        let max_output_tokens = input
            .max_output_tokens
            .map(|value| {
                NonZeroU64::new(value).ok_or_else(|| {
                    ConfigError::Validation(
                        "model max_output_tokens must be nonzero when set".to_owned(),
                    )
                })
            })
            .transpose()?;
        models.push(Model {
            id: input.id,
            max_output_tokens,
        });
    }
    Ok(models)
}

fn validate_roots(inputs: Vec<RootInput>, models: &[Model]) -> Result<Vec<Root>, ConfigError> {
    if inputs.len() > MAX_ROOTS {
        return invalid("configuration supports at most 16 roots");
    }
    let model_ids: HashSet<&str> = models.iter().map(|model| model.id.as_str()).collect();
    let mut root_ids = HashSet::with_capacity(inputs.len());
    let mut roots = Vec::with_capacity(inputs.len());

    for input in inputs {
        validate_root_id(&input.id)?;
        if !root_ids.insert(input.id.clone()) {
            return invalid("duplicate root id");
        }
        if input.endpoints.is_empty() {
            return invalid("each root must enable at least one endpoint");
        }
        if input.models.is_empty() {
            return invalid("each root must enable at least one model");
        }
        for model in &input.models {
            if !model_ids.contains(model.as_str()) {
                return invalid("root references an unknown model");
            }
        }
        let endpoints = input
            .endpoints
            .iter()
            .map(|value| validate_endpoint(value))
            .collect::<Result<Vec<_>, _>>()?;
        roots.push(Root {
            id: input.id,
            endpoints,
            models: input.models,
        });
    }
    Ok(roots)
}

fn validate_root_id(id: &str) -> Result<(), ConfigError> {
    let mut bytes = id.bytes();
    let starts_safely = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric());
    let rest_is_safe =
        bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !starts_safely || !rest_is_safe || id.len() > MAX_ROOT_ID_BYTES {
        return invalid(
            "root id must be 1-64 ASCII bytes, start with a letter or digit, and contain only letters, digits, '-' or '_'",
        );
    }
    Ok(())
}

fn validate_endpoint(value: &str) -> Result<Endpoint, ConfigError> {
    match value {
        "chat/completions" => Ok(Endpoint::ChatCompletions),
        "responses" => Ok(Endpoint::Responses),
        "messages" => Ok(Endpoint::Messages),
        "messages/count_tokens" => Ok(Endpoint::CountTokens),
        "models" => Ok(Endpoint::Models),
        _ => invalid("unsupported endpoint"),
    }
}

fn required_nonempty(field: &str, value: Option<String>) -> Result<String, ConfigError> {
    let value = value.ok_or_else(|| ConfigError::Validation(format!("{field} is required")))?;
    if value.is_empty() {
        return invalid(format!("{field} must not be empty"));
    }
    Ok(value)
}

fn is_http_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

pub(crate) fn is_env_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    let starts_safely = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    starts_safely && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_transport_reserved_header(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-llmgw-token"
            | "x-llmgw-control-token"
    )
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Validation(message.into()))
}

/// Typed Config bypasses TOML; the public server still enforces bounded route identities.
pub(crate) fn registered_root_ids(roots: &[Root]) -> Result<(), ConfigError> {
    if roots.len() > MAX_ROOTS {
        return invalid("configuration supports at most 16 roots");
    }
    let mut ids = HashSet::new();
    for root in roots {
        validate_root_id(&root.id)?;
        if !ids.insert(root.id.as_str()) {
            return invalid("duplicate root id");
        }
    }
    Ok(())
}
