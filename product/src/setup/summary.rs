use super::{ClientIntent, Error, RuntimeImpact, SetupDraft, absolute, pending_path};
use crate::config::{Auth, Limit, StatePaths};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleUrls {
    pub openai: String,
    pub messages: String,
    pub messages_request: String,
}

pub fn example_urls(listen_base: &str, root: &str) -> Result<ExampleUrls, Error> {
    if root.is_empty()
        || root.len() > 64
        || !root.as_bytes()[0].is_ascii_alphanumeric()
        || !root
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(Error::message("root id is not registrable"));
    }
    let mut base = url::Url::parse(listen_base)
        .map_err(|_| Error::message("listen base must be an absolute URL"))?;
    base.set_path("");
    {
        let mut parts = base.path_segments_mut().expect("HTTP base URL");
        parts.clear().push("r").push(root);
    }
    let messages = base.as_str().trim_end_matches('/').to_owned();
    let openai = format!("{messages}/v1");
    let messages_request = format!("{messages}/v1/messages");
    Ok(ExampleUrls {
        openai,
        messages,
        messages_request,
    })
}

pub fn render(
    draft: &SetupDraft,
    config_path: &Path,
    runtime_impact: &RuntimeImpact,
) -> Result<String, Error> {
    draft.validate()?;
    let config_path = absolute(config_path)?;
    let state = StatePaths::from_config_path(&config_path)?;
    let pending = pending_path(&config_path)?;
    let root = draft.config.roots.first().expect("validated route");
    let listen = format!("http://{}", draft.config.listen);
    let urls = example_urls(&listen, &root.id)?;
    let auth = match &draft.config.upstream.auth {
        Auth::None => "none".to_owned(),
        Auth::Forward => "forward (network doctor cannot supply incoming client auth)".to_owned(),
        Auth::Env { header, name } => format!(
            "env {name} -> {header}; value hidden; complete header value required; current shell: {}; next-login credential: not configured",
            if std::env::var_os(name).is_some() {
                "available"
            } else {
                "unavailable"
            },
        ),
    };
    let known = |limit: &Limit| matches!(limit, Limit::Known(_));
    let quota = if draft.shared_with_other_pcs
        || draft.separate_input_output
        || !known(&draft.config.quota.rpm)
        || !known(&draft.config.quota.tpm)
    {
        "estimated"
    } else {
        "configured"
    };
    let clients = draft
        .clients
        .iter()
        .map(|client| match client {
            ClientIntent::Pi => "Pi",
            ClientIntent::ClaudeCode => "Claude Code",
            ClientIntent::Codex => "Codex",
            ClientIntent::Manual => "manual",
        })
        .collect::<Vec<_>>()
        .join(", ");
    let original_upstream = draft
        .original
        .as_ref()
        .map_or("new configuration".to_owned(), |(config, _)| {
            config.upstream.api_base.to_string()
        });
    let fingerprint_impact = match runtime_impact {
        RuntimeImpact::NotInspected {
            desired_fingerprint,
        } => format!(
            "desired {desired_fingerprint}; worker fingerprint not inspected; setup authenticates it before any start or restart decision"
        ),
        RuntimeImpact::NewConfig {
            desired_fingerprint,
        } => format!(
            "desired {desired_fingerprint}; new config with no existing worker; Save and start verifies authenticated readiness"
        ),
        RuntimeImpact::Stopped {
            desired_fingerprint,
        } => format!(
            "desired {desired_fingerprint}; lifecycle state is stopped; Save and start starts this configuration and verifies authenticated readiness"
        ),
        RuntimeImpact::Matching { fingerprint } => format!(
            "desired {fingerprint}; authenticated running worker matches; no restart required; Save and start uses authenticated idempotent on and preserves worker identity"
        ),
        RuntimeImpact::RestartRequired {
            desired_fingerprint,
            worker_fingerprint,
        } => format!(
            "desired {desired_fingerprint}; authenticated running worker {worker_fingerprint} differs; Save only preserves the worker and leaves restart pending; Save and start explicitly restarts, cancels queued requests, drains active requests for up to 10 seconds, and verifies readiness for the desired fingerprint"
        ),
        RuntimeImpact::Unverified {
            desired_fingerprint,
        } => format!(
            "desired {desired_fingerprint}; worker fingerprint could not be authenticated; Save only preserves the worker, while Save and start refuses before writing or restarting"
        ),
    };
    let output_bound = draft.config.models[0].max_output_tokens.map_or_else(
        || "none; request-supplied caps are used when present, and known TPM generation admission requires one because the gateway does not invent or inject a cap".to_owned(),
        |value| format!("{}; user-supplied accounting fallback used only when a request omits its cap, not a verified provider limit, and not injected or enforced upstream", value.get()),
    );
    let transport = format!(
        "proxy: {}; TLS: system roots{}; verification remains enabled",
        if draft.config.upstream.proxy.is_some() {
            "explicit (loopback bypassed)"
        } else {
            "disabled"
        },
        draft
            .config
            .upstream
            .ca_bundle
            .as_ref()
            .map_or(String::new(), |path| format!(
                " plus explicit CA {}",
                path.display()
            )),
    );
    Ok(format!(
        "config: {}\nstate: {} (final apply initializes protected local data/control tokens; preview and cancel create no state)\nsetup pending metadata: {} (intent only; no credentials)\nupstream original: {}\nupstream new normalized: {}\n{}\nOpenAI client base: {}\nMessages client base: {} (client appends /v1/messages -> {})\nprotocols: {:?}\nmodel: {}; reservation output bound: {}; listing: {}; capabilities: {}\nquota: {quota}; shared with other PCs: {}; separate input/output contract: {}\nauth: {auth}\nquota startup hold: known RPM/TPM can hold admission for up to 60 seconds after readiness\nlogin requested: {}; pending, registration not applied; after config save, exact OS target preview and separate confirmation\nclients: {}; pending, client files not changed\nfingerprint impact: {}\nfairness: configured route root {}; sessions sharing this root share one budget\n",
        config_path.display(),
        state.directory.display(),
        pending.display(),
        original_upstream,
        draft.config.upstream.api_base,
        transport,
        urls.openai,
        urls.messages,
        urls.messages_request,
        root.endpoints,
        draft.config.models[0].id,
        output_bound,
        if draft.listing_verified {
            "verified GET only"
        } else {
            "unverified/manual"
        },
        if draft.capabilities_verified {
            "verified"
        } else {
            "unverified"
        },
        draft.shared_with_other_pcs,
        draft.separate_input_output,
        if draft.login_requested { "yes" } else { "no" },
        if clients.is_empty() { "none" } else { &clients },
        fingerprint_impact,
        root.id,
    ))
}
