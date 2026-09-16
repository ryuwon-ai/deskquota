//! First-run configuration. Prompt collection is separate from the typed draft and apply step.
mod document;
mod network;
mod persist;
pub mod prompts;
pub mod summary;
pub mod validate;
pub use network::{
    InferenceResult, ListRequest, ModelListing, fetch_models, list_models, smoke_inference,
};
pub use persist::{ApplyMode, ApplyResult, RuntimeImpact, absolute, apply, runtime_impact};
use persist::{PendingSetup, pending_path};

use crate::config::{
    Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, LoadedConfig, Model, Quota, Root,
    Upstream,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    num::NonZeroU64,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug)]
pub struct Error(String);

impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<crate::config::ConfigError> for Error {
    fn from(value: crate::config::ConfigError) -> Self {
        Self(value.to_string())
    }
}

impl From<crate::lifecycle::Error> for Error {
    fn from(value: crate::lifecycle::Error) -> Self {
        Self(value.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentPreset {
    LocalLlm,
    Corporate,
    ExternalApi,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionAnswers {
    pub api_base: String,
    pub endpoints: Vec<Endpoint>,
    pub auth: Auth,
    pub proxy: Option<String>,
    pub ca_bundle: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelAnswers {
    pub id: String,
    pub max_output_tokens: Option<u64>,
    pub input_estimator: Option<crate::input_estimate::InputEstimator>,
    pub input_token_overhead: Option<u64>,
    pub listing_verified: bool,
    pub capabilities_verified: bool,
}

impl ModelAnswers {
    pub fn manual(id: impl Into<String>, max_output_tokens: Option<u64>) -> Self {
        Self {
            id: id.into(),
            max_output_tokens,
            input_estimator: None,
            input_token_overhead: None,
            listing_verified: false,
            capabilities_verified: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LimitAnswer {
    Known(NonZeroU64),
    Unknown,
    Unlimited,
}

impl LimitAnswer {
    pub fn known(value: u64) -> Result<Self, Error> {
        NonZeroU64::new(value)
            .map(Self::Known)
            .ok_or_else(|| Error::message("known quota must be nonzero"))
    }

    pub fn to_limit(&self) -> Limit {
        match self {
            Self::Known(value) => Limit::Known(*value),
            Self::Unknown => Limit::Unknown,
            Self::Unlimited => Limit::Unlimited,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuotaAnswers {
    pub rpm: LimitAnswer,
    pub tpm: LimitAnswer,
    pub shared_with_other_pcs: bool,
    pub separate_input_output: bool,
    pub concurrency: u8,
    pub startup_hold_secs: u64,
    pub cache: Option<crate::config::CacheConfig>,
}

impl QuotaAnswers {
    pub fn validate(&self) -> Result<(), Error> {
        if !(crate::config::MIN_CONCURRENCY..=crate::config::MAX_CONCURRENCY)
            .contains(&self.concurrency)
        {
            return Err(Error::message("concurrency must be between 1 and 16"));
        }
        if self.cache.is_some_and(|cache| !cache.is_valid()) {
            return Err(Error::message(
                "cache ttl_secs must be 1..3600 and max_history must be 1..64",
            ));
        }
        if self.startup_hold_secs > 3600 {
            return Err(Error::message(
                "startup_hold_secs must be between 0 and 3600",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunAnswers {
    pub port: u16,
    pub login_requested: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientIntent {
    Pi,
    ClaudeCode,
    Codex,
    Manual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolAnswers {
    pub clients: Vec<ClientIntent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupDraft {
    environment: EnvironmentPreset,
    config: Config,
    shared_with_other_pcs: bool,
    separate_input_output: bool,
    login_requested: bool,
    clients: Vec<ClientIntent>,
    listing_verified: bool,
    capabilities_verified: bool,
    connection_error: Option<String>,
    original: Option<(Config, Vec<u8>)>,
    original_pending: Option<Option<Vec<u8>>>,
}

impl SetupDraft {
    pub fn new(environment: EnvironmentPreset) -> Self {
        Self {
            environment,
            config: Config {
                listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4141),
                concurrency: 1,
                startup_hold_secs: 60,
                cache: None,
                cancel_policy: CancelPolicy::Drain,
                accounting: Accounting::Actual,
                retry_transient_429: false,
                upstream: Upstream {
                    api_base: "http://127.0.0.1:11434/v1".parse().expect("static URL"),
                    auth: Auth::None,
                    proxy: None,
                    ca_bundle: None,
                },
                quota: Quota {
                    rpm: Limit::Unknown,
                    tpm: Limit::Unknown,
                },
                models: Vec::new(),
                roots: Vec::new(),
            },
            shared_with_other_pcs: false,
            separate_input_output: false,
            login_requested: false,
            clients: Vec::new(),
            listing_verified: false,
            capabilities_verified: false,
            connection_error: None,
            original: None,
            original_pending: None,
        }
    }

    pub fn from_existing(config: &Config) -> Self {
        Self::from_config(config.clone(), None)
    }

    pub fn from_loaded(loaded: &LoadedConfig) -> Result<Self, Error> {
        let mut draft = Self::from_config(
            loaded.config.clone(),
            Some((loaded.config.clone(), loaded.source_bytes.clone())),
        );
        draft.original_pending = Some(None);
        let path = pending_path(&loaded.source_path)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(Error::message(
                        "setup pending metadata must be a regular file, not a link",
                    ));
                }
                let bytes = fs::read(&path)?;
                if bytes.len() > 16_384 {
                    return Err(Error::message("setup pending metadata exceeds 16384 bytes"));
                }
                let pending: PendingSetup = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::message("setup pending metadata is invalid"))?;
                if pending.version != 1 {
                    return Err(Error::message(
                        "setup pending metadata version is unsupported",
                    ));
                }
                draft.shared_with_other_pcs = pending.shared_with_other_pcs;
                draft.separate_input_output = pending.separate_input_output;
                draft.login_requested = pending.login_requested;
                draft.clients = pending.clients;
                draft.original_pending = Some(Some(bytes));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(draft)
    }

    fn from_config(config: Config, original: Option<(Config, Vec<u8>)>) -> Self {
        Self {
            environment: EnvironmentPreset::ExternalApi,
            config,
            shared_with_other_pcs: false,
            separate_input_output: false,
            login_requested: false,
            clients: Vec::new(),
            listing_verified: false,
            capabilities_verified: false,
            connection_error: None,
            original,
            original_pending: None,
        }
    }

    pub fn connection(mut self, answers: ConnectionAnswers) -> Self {
        self.connection_error = None;
        match validate::api_base(&answers.api_base) {
            Ok(url) => self.config.upstream.api_base = url,
            Err(error) => self.connection_error = Some(error.to_string()),
        }
        self.config.upstream.auth = answers.auth;
        self.config.upstream.proxy = match answers.proxy.as_deref().map(validate::proxy).transpose()
        {
            Ok(proxy) => proxy,
            Err(error) => {
                self.connection_error
                    .get_or_insert_with(|| error.to_string());
                None
            }
        };
        self.config.upstream.ca_bundle =
            match answers.ca_bundle.map(|path| absolute(&path)).transpose() {
                Ok(path) => path,
                Err(error) => {
                    self.connection_error
                        .get_or_insert_with(|| error.to_string());
                    None
                }
            };
        if let Some(root) = self.config.roots.first_mut() {
            root.endpoints = answers.endpoints;
        } else if !answers.endpoints.is_empty() {
            self.config.roots.push(Root {
                id: "default".into(),
                endpoints: answers.endpoints,
                models: Vec::new(),
            });
        }
        self
    }

    pub fn models(mut self, answers: ModelAnswers) -> Self {
        let max_output_tokens = answers.max_output_tokens.and_then(NonZeroU64::new);
        let existing = self
            .config
            .models
            .iter()
            .find(|model| model.id == answers.id);
        let input_estimator = answers
            .input_estimator
            .or_else(|| existing.map(|model| model.input_estimator))
            .unwrap_or_default();
        let input_token_overhead = answers
            .input_token_overhead
            .or_else(|| {
                existing
                    .filter(|model| model.input_estimator == input_estimator)
                    .map(|model| model.input_token_overhead)
            })
            .unwrap_or_else(|| input_estimator.default_overhead());
        let model = Model {
            id: answers.id.clone(),
            max_output_tokens,
            input_estimator,
            input_token_overhead,
        };
        if let Some(first) = self.config.models.first_mut() {
            let old_id = std::mem::replace(first, model).id;
            for root in &mut self.config.roots {
                for id in &mut root.models {
                    if *id == old_id {
                        *id = answers.id.clone();
                    }
                }
            }
        } else {
            self.config.models.push(model);
        }
        if let Some(root) = self.config.roots.first_mut() {
            if root.models.is_empty() {
                root.models.push(answers.id);
            }
        }
        self.listing_verified = answers.listing_verified;
        self.capabilities_verified = answers.capabilities_verified;
        self
    }

    pub fn quota(mut self, answers: QuotaAnswers) -> Self {
        self.config.quota = Quota {
            rpm: answers.rpm.to_limit(),
            tpm: answers.tpm.to_limit(),
        };
        self.config.concurrency = answers.concurrency;
        self.config.startup_hold_secs = answers.startup_hold_secs;
        self.config.cache = answers.cache;
        self.shared_with_other_pcs = answers.shared_with_other_pcs;
        self.separate_input_output = answers.separate_input_output;
        self
    }

    pub fn run(mut self, answers: RunAnswers) -> Self {
        self.config.listen.set_port(answers.port);
        self.login_requested = answers.login_requested;
        self
    }

    pub fn tools(mut self, answers: ToolAnswers) -> Self {
        self.clients = answers.clients;
        self
    }

    /// Add one reviewed fixed client route without replacing existing routes,
    /// models, comments, or quota policy.
    pub fn ensure_client_route(
        mut self,
        root_id: &str,
        model_id: &str,
        endpoint: Endpoint,
    ) -> Result<Self, Error> {
        if root_id.is_empty() || model_id.is_empty() {
            return Err(Error::message("client root and model must be nonempty"));
        }
        if !self.config.models.iter().any(|model| model.id == model_id) {
            self.config.models.push(Model {
                id: model_id.to_owned(),
                max_output_tokens: None,
                input_estimator: Default::default(),
                input_token_overhead: 0,
            });
        }
        if let Some(root) = self.config.roots.iter_mut().find(|root| root.id == root_id) {
            if !root.endpoints.contains(&endpoint) {
                root.endpoints.push(endpoint);
            }
            if !root.models.iter().any(|model| model == model_id) {
                root.models.push(model_id.to_owned());
            }
        } else {
            self.config.roots.push(Root {
                id: root_id.to_owned(),
                endpoints: vec![endpoint],
                models: vec![model_id.to_owned()],
            });
        }
        self.validate()?;
        Ok(self)
    }

    pub fn concurrency(&self) -> u8 {
        self.config.concurrency
    }
    pub fn login_requested(&self) -> bool {
        self.login_requested
    }
    pub fn endpoints(&self) -> &[Endpoint] {
        self.config
            .roots
            .first()
            .map_or(&[], |root| root.endpoints.as_slice())
    }
    pub const fn local_model_started_or_downloaded(&self) -> bool {
        false
    }
    pub fn accounting(&self) -> Accounting {
        self.config.accounting
    }
    pub fn retry_transient_429(&self) -> bool {
        self.config.retry_transient_429
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn shared_with_other_pcs(&self) -> bool {
        self.shared_with_other_pcs
    }
    pub fn separate_input_output(&self) -> bool {
        self.separate_input_output
    }
    pub fn clients(&self) -> &[ClientIntent] {
        &self.clients
    }

    pub fn validate(&self) -> Result<(), Error> {
        if let Some(error) = &self.connection_error {
            return Err(Error::message(error.clone()));
        }
        if self.config.models.is_empty() || self.config.roots.is_empty() {
            return Err(Error::message("at least one model and route are required"));
        }
        if self
            .config
            .roots
            .iter()
            .any(|root| root.endpoints.is_empty())
        {
            return Err(Error::message(
                "at least one supported endpoint must be selected",
            ));
        }
        if !(crate::config::MIN_CONCURRENCY..=crate::config::MAX_CONCURRENCY)
            .contains(&self.config.concurrency)
        {
            return Err(Error::message("concurrency must be between 1 and 16"));
        }
        if self.config.listen.port() == 0 {
            return Err(Error::message("listen port must be between 1 and 65535"));
        }
        let rendered = serialize_config(&self.config)?;
        crate::config::parse(rendered.as_bytes())?;
        Ok(())
    }

    pub fn render_config(&self) -> Result<String, Error> {
        self.validate()?;
        if let Some((config, bytes)) = &self.original {
            if config == &self.config {
                return String::from_utf8(bytes.clone())
                    .map_err(|_| Error::message("existing configuration is not UTF-8"));
            }
            return document::render_preserving(bytes, config, &self.config);
        }
        serialize_config(&self.config)
    }

    pub fn summary(&self, config_path: &Path) -> Result<String, Error> {
        let desired_fingerprint =
            crate::config::ConfigFingerprint::from_bytes(self.render_config()?.as_bytes())
                .as_str()
                .to_owned();
        summary::render(
            self,
            config_path,
            &RuntimeImpact::NotInspected {
                desired_fingerprint,
            },
        )
    }

    pub fn summary_with_runtime(
        &self,
        config_path: &Path,
        impact: &RuntimeImpact,
    ) -> Result<String, Error> {
        summary::render(self, config_path, impact)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Environment,
    Connection,
    Models,
    Quota,
    Run,
    Tools,
    Apply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Flow {
    SetEnvironment(EnvironmentPreset),
    SetConnection(ConnectionAnswers),
    SetModels(ModelAnswers),
    SetQuota(QuotaAnswers),
    SetRun(RunAnswers),
    SetTools(ToolAnswers),
    Apply(ApplyMode),
    Keep,
    Back,
    Cancel,
}

pub trait PromptIo {
    fn prompt(&mut self, step: Step, draft: &SetupDraft) -> Result<Flow, Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupOutcome {
    Cancelled,
    Ready {
        draft: Box<SetupDraft>,
        mode: ApplyMode,
    },
}

pub fn run_wizard(
    io: &mut impl PromptIo,
    existing: Option<&LoadedConfig>,
) -> Result<SetupOutcome, Error> {
    let mut draft = match existing {
        Some(loaded) => SetupDraft::from_loaded(loaded)?,
        None => SetupDraft::new(EnvironmentPreset::LocalLlm),
    };
    let steps = [
        Step::Environment,
        Step::Connection,
        Step::Models,
        Step::Quota,
        Step::Run,
        Step::Tools,
        Step::Apply,
    ];
    let mut index = 0usize;
    loop {
        let answer = match io.prompt(steps[index], &draft) {
            Err(error) if error.to_string() == "cancel" => return Ok(SetupOutcome::Cancelled),
            result => result?,
        };
        match answer {
            Flow::Cancel => return Ok(SetupOutcome::Cancelled),
            Flow::Back => index = index.saturating_sub(1),
            Flow::Keep => index += 1,
            Flow::SetEnvironment(value) if steps[index] == Step::Environment => {
                draft.environment = value;
                index += 1;
            }
            Flow::SetConnection(value) if steps[index] == Step::Connection => {
                draft = draft.connection(value);
                index += 1;
            }
            Flow::SetModels(value) if steps[index] == Step::Models => {
                draft = draft.models(value);
                index += 1;
            }
            Flow::SetQuota(value) if steps[index] == Step::Quota => {
                value.validate()?;
                draft = draft.quota(value);
                index += 1;
            }
            Flow::SetRun(value) if steps[index] == Step::Run => {
                draft = draft.run(value);
                index += 1;
            }
            Flow::SetTools(value) if steps[index] == Step::Tools => {
                draft = draft.tools(value);
                index += 1;
            }
            Flow::Apply(mode) if steps[index] == Step::Apply => {
                draft.validate()?;
                return Ok(SetupOutcome::Ready {
                    draft: Box::new(draft),
                    mode,
                });
            }
            _ => {
                return Err(Error::message(
                    "prompt returned an answer for the wrong setup step",
                ));
            }
        }
    }
}

#[derive(Serialize)]
struct ConfigOutput<'a> {
    listen: String,
    concurrency: u8,
    startup_hold_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<crate::config::CacheConfig>,
    cancel_policy: &'static str,
    accounting: &'static str,
    retry_transient_429: bool,
    upstream: UpstreamOutput<'a>,
    quota: QuotaOutput,
    models: Vec<ModelOutput<'a>>,
    roots: Vec<RootOutput<'a>>,
}

#[derive(Serialize)]
struct UpstreamOutput<'a> {
    api_base: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ca_bundle: Option<&'a Path>,
    auth: AuthOutput<'a>,
}

#[derive(Serialize)]
struct AuthOutput<'a> {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    header: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

#[derive(Serialize)]
struct QuotaOutput {
    rpm: LimitOutput,
    tpm: LimitOutput,
}
#[derive(Serialize)]
struct LimitOutput {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
}
#[derive(Serialize)]
struct ModelOutput<'a> {
    id: &'a str,
    max_output_tokens: Option<u64>,
    input_estimator: crate::input_estimate::InputEstimator,
    input_token_overhead: u64,
}
#[derive(Serialize)]
struct RootOutput<'a> {
    id: &'a str,
    endpoints: Vec<&'static str>,
    models: &'a [String],
}

fn serialize_config(config: &Config) -> Result<String, Error> {
    let auth = match &config.upstream.auth {
        Auth::Forward => AuthOutput {
            mode: "forward",
            header: None,
            name: None,
        },
        Auth::None => AuthOutput {
            mode: "none",
            header: None,
            name: None,
        },
        Auth::Env { header, name } => AuthOutput {
            mode: "env",
            header: Some(header),
            name: Some(name),
        },
    };
    let limit = |value: &Limit| match value {
        Limit::Known(value) => LimitOutput {
            kind: "known",
            value: Some(value.get()),
        },
        Limit::Unknown => LimitOutput {
            kind: "unknown",
            value: None,
        },
        Limit::Unlimited => LimitOutput {
            kind: "unlimited",
            value: None,
        },
    };
    let endpoint = |value: &Endpoint| match value {
        Endpoint::ChatCompletions => "chat/completions",
        Endpoint::Responses => "responses",
        Endpoint::Messages => "messages",
        Endpoint::CountTokens => "messages/count_tokens",
        Endpoint::Models => "models",
    };
    let output = ConfigOutput {
        listen: config.listen.to_string(),
        concurrency: config.concurrency,
        startup_hold_secs: config.startup_hold_secs,
        cache: config.cache,
        cancel_policy: match config.cancel_policy {
            CancelPolicy::Drain => "drain",
            CancelPolicy::Close => "close",
        },
        accounting: match config.accounting {
            Accounting::Reserved => "reserved",
            Accounting::Actual => "actual",
        },
        retry_transient_429: config.retry_transient_429,
        upstream: UpstreamOutput {
            api_base: config.upstream.api_base.as_str(),
            proxy: config.upstream.proxy.as_ref().map(url::Url::as_str),
            ca_bundle: config.upstream.ca_bundle.as_deref(),
            auth,
        },
        quota: QuotaOutput {
            rpm: limit(&config.quota.rpm),
            tpm: limit(&config.quota.tpm),
        },
        models: config
            .models
            .iter()
            .map(|model| ModelOutput {
                id: &model.id,
                max_output_tokens: model.max_output_tokens.map(NonZeroU64::get),
                input_estimator: model.input_estimator,
                input_token_overhead: model.input_token_overhead,
            })
            .collect(),
        roots: config
            .roots
            .iter()
            .map(|root| RootOutput {
                id: &root.id,
                endpoints: root.endpoints.iter().map(endpoint).collect(),
                models: &root.models,
            })
            .collect(),
    };
    toml_edit::ser::to_string_pretty(&output).map_err(|error| Error::message(error.to_string()))
}

pub use summary::{ExampleUrls, example_urls};

pub fn cancelled_exit() -> ExitCode {
    ExitCode::from(130)
}
