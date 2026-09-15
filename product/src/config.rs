mod validate;
pub(crate) use validate::{is_env_name, registered_root_ids};

pub(crate) const MIN_CONCURRENCY: u8 = 1;
pub(crate) const MAX_CONCURRENCY: u8 = 16;

use std::fmt;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Limit {
    Known(NonZeroU64),
    Unknown,
    Unlimited,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Auth {
    Forward,
    Env { header: String, name: String },
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accounting {
    Reserved,
    Actual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelPolicy {
    Drain,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endpoint {
    ChatCompletions,
    Responses,
    Messages,
    CountTokens,
    Models,
}

impl Endpoint {
    pub const fn method(self) -> Method {
        match self {
            Self::Models => Method::Get,
            Self::ChatCompletions | Self::Responses | Self::Messages | Self::CountTokens => {
                Method::Post
            }
        }
    }

    pub const fn path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat/completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::CountTokens => "messages/count_tokens",
            Self::Models => "models",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Quota {
    pub rpm: Limit,
    pub tpm: Limit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Model {
    pub id: String,
    pub max_output_tokens: Option<NonZeroU64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Root {
    pub id: String,
    pub endpoints: Vec<Endpoint>,
    pub models: Vec<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct Upstream {
    pub api_base: url::Url,
    pub auth: Auth,
    pub proxy: Option<url::Url>,
    pub ca_bundle: Option<PathBuf>,
}

impl fmt::Debug for Upstream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Upstream")
            .field("api_base", &"<redacted>")
            .field("auth", &self.auth)
            .field("proxy", &self.proxy.as_ref().map(|_| "<configured>"))
            .field("ca_bundle", &self.ca_bundle)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CacheConfig {
    pub ttl_secs: u64,
    pub max_history: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl_secs: 300,
            max_history: 3,
        }
    }
}

impl CacheConfig {
    pub fn is_valid(self) -> bool {
        (1..=3600).contains(&self.ttl_secs) && (1..=64).contains(&self.max_history)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub listen: SocketAddr,
    pub concurrency: u8,
    pub startup_hold_secs: u64,
    pub cache: Option<CacheConfig>,
    pub cancel_policy: CancelPolicy,
    pub accounting: Accounting,
    pub retry_transient_429: bool,
    pub upstream: Upstream,
    pub quota: Quota,
    pub models: Vec<Model>,
    pub roots: Vec<Root>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatePaths {
    pub directory: PathBuf,
    pub control_token: PathBuf,
    pub runtime_state: PathBuf,
}

impl StatePaths {
    /// The caller must provide the canonical path, independently of current TOML validity.
    pub fn path_hash(canonical_path: &Path) -> String {
        ConfigFingerprint::from_bytes(canonical_path.as_os_str().as_encoded_bytes()).0
    }

    pub fn from_config_path(config_path: &Path) -> Result<Self, ConfigError> {
        let parent = config_path.parent().ok_or_else(|| {
            ConfigError::Validation("config path must have a parent directory".to_owned())
        })?;
        let directory = parent.join(format!(".llmgw-{}", Self::path_hash(config_path)));
        Ok(Self {
            control_token: directory.join("control-token"),
            runtime_state: directory.join("runtime.json"),
            directory,
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ConfigFingerprint(String);

impl ConfigFingerprint {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut encoded = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use fmt::Write as _;
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
        }
        Self(encoded)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ConfigFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ConfigFingerprint")
            .field(&self.0)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedConfig {
    pub config: Config,
    pub source_path: PathBuf,
    pub fingerprint: ConfigFingerprint,
    pub state_paths: StatePaths,
    pub source_bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        line: Option<usize>,
        column: Option<usize>,
    },
    Validation(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} config {}: {source}",
                path.display()
            ),
            Self::Parse {
                line: Some(line),
                column: Some(column),
            } => write!(
                formatter,
                "invalid configuration syntax or shape at line {line}, column {column}"
            ),
            Self::Parse { .. } => formatter.write_str("invalid configuration syntax or shape"),
            Self::Validation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { .. } | Self::Validation(_) => None,
        }
    }
}

impl LoadedConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let requested_path = path.as_ref();
        let source_path = fs::canonicalize(requested_path).map_err(|source| ConfigError::Io {
            operation: "resolve",
            path: requested_path.to_owned(),
            source,
        })?;
        let bytes = fs::read(&source_path).map_err(|source| ConfigError::Io {
            operation: "read",
            path: source_path.clone(),
            source,
        })?;
        let fingerprint = ConfigFingerprint::from_bytes(&bytes);
        let config = parse(&bytes)?;
        let state_paths = StatePaths::from_config_path(&source_path)?;

        Ok(Self {
            config,
            source_path,
            fingerprint,
            state_paths,
            source_bytes: bytes,
        })
    }
}

/// Parse and validate configuration bytes without filesystem, credentials, or network access.
pub fn parse(bytes: &[u8]) -> Result<Config, ConfigError> {
    let input: ConfigInput = toml_edit::de::from_slice(bytes).map_err(|error| {
        let (line, column) = error
            .span()
            .map(|span| line_and_column(bytes, span.start))
            .map_or((None, None), |(line, column)| (Some(line), Some(column)));
        ConfigError::Parse { line, column }
    })?;
    validate::validate(input)
}

fn line_and_column(bytes: &[u8], offset: usize) -> (usize, usize) {
    let offset = offset.min(bytes.len());
    let prefix = &bytes[..offset];
    let line = 1 + prefix.iter().filter(|byte| **byte == b'\n').count();
    let line_start = prefix
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    let column = offset - line_start + 1;
    (line, column)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigInput {
    listen: Option<String>,
    concurrency: Option<u8>,
    startup_hold_secs: Option<u64>,
    cache: Option<CacheConfig>,
    cancel_policy: Option<String>,
    accounting: Option<String>,
    retry_transient_429: Option<bool>,
    upstream: UpstreamInput,
    quota: QuotaInput,
    models: Vec<ModelInput>,
    roots: Vec<RootInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamInput {
    api_base: String,
    auth: AuthInput,
    proxy: Option<String>,
    ca_bundle: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthInput {
    mode: String,
    header: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuotaInput {
    rpm: LimitInput,
    tpm: LimitInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitInput {
    kind: String,
    value: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelInput {
    id: String,
    max_output_tokens: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootInput {
    id: String,
    endpoints: Vec<String>,
    models: Vec<String>,
}
