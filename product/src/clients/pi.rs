use super::*;
use serde_json::json;

pub(super) fn patches(
    request: &ProfileRequest,
    config_dir: &Path,
) -> Result<AdapterPatches, Error> {
    version_is(&request.installed_version, "0.84.2", "Pi")?;
    if request.protocol != Protocol::OpenAiCompletions {
        return Err(Error::message("Pi 0.84.2 requires openai_completions"));
    }
    if request.project_local.is_some() {
        return Err(Error::message("Pi project-local profile is unsupported"));
    }
    let agent = config_dir;
    let models_path = agent.join("models.json");
    let ownership_requirement = owned_provider_requirement(request, &models_path)?;
    let base = format!(
        "{}/r/{}/v1",
        request.gateway_origin.trim_end_matches('/'),
        request.root
    );
    let context_window = request.metadata.context_window.unwrap_or(128_000);
    let max_tokens = request.metadata.max_output_tokens.unwrap_or(16_384);
    let mut warnings = Vec::new();
    if request.metadata.context_window.is_none() || request.metadata.max_output_tokens.is_none() {
        warnings.push(
            "Pi client defaults 128000/16384 were written because upstream limits are unverified; these are not verified provider capabilities"
                .into(),
        );
    }
    let model = json!({
        "id": request.model,
        "input": ["text"],
        "contextWindow": context_window,
        "maxTokens": max_tokens,
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
    });
    let provider = json!({
        "baseUrl": base,
        "api": "openai-completions",
        "apiKey": if request.upstream_auth == crate::config::Auth::Forward { format!("${}", request.client_key_env) } else { "llmgw-local-only".to_owned() },
        "headers": {"X-LLMGW-Client": "pi"},
        "models": [model]
    });
    let model_edits = vec![Edit::Set {
        key: key(&["providers", "llmgw"])?,
        value: EditValue::Public(provider),
    }];
    let mut patches = vec![patch(
        models_path,
        Format::JsonWithComments,
        model_edits,
        warnings.clone(),
    )?];
    if request.set_default {
        patches.push(patch(
            agent.join("settings.json"),
            Format::StrictJson,
            vec![
                set_public(&["defaultProvider"], json!("llmgw"))?,
                set_public(&["defaultModel"], json!(request.model))?,
            ],
            Vec::new(),
        )?);
    }
    let plan_checks = match ownership_requirement {
        Some(requirement) => vec![PlanCheck::OwnedProvider {
            journal_path: absolute(&request.journal_path)?,
            requirement,
        }],
        None => Vec::new(),
    };
    Ok((patches, base, Scope::UserPrivate, warnings, plan_checks))
}

fn owned_provider_requirement(
    request: &ProfileRequest,
    path: &Path,
) -> Result<Option<crate::config_patch::OwnedKeyRequirement>, Error> {
    let (_, bytes) = snapshot(path)?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let value = crate::config_patch::document::parse_document(Format::JsonWithComments, &bytes)?;
    if value.pointer("/providers/llmgw").is_none() {
        return Ok(None);
    }
    let provider_key = key(&["providers", "llmgw"])?;
    match crate::config_patch::journal::owned_key_current_matches(
        &request.journal_path,
        path,
        Format::JsonWithComments,
        &provider_key,
    )? {
        Some(true) => Ok(Some(crate::config_patch::OwnedKeyRequirement {
            resource_path: path.to_owned(),
            format: Format::JsonWithComments,
            key: provider_key,
        })),
        Some(false) => Err(Error::message(
            "existing llmgw provider changed after the reviewed connection; reconnect refused and disconnect will preserve it as a conflict",
        )),
        None => Err(Error::message(
            "existing llmgw provider is not owned by this connection journal; disconnect the prior managed profile or choose another provider name",
        )),
    }
}
