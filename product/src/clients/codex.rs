use super::*;
use serde_json::json;

pub(super) fn patches(
    request: &ProfileRequest,
    config_dir: &Path,
) -> Result<AdapterPatches, Error> {
    version_is(&request.installed_version, "0.154.0", "Codex")?;
    if request.protocol != Protocol::OpenAiResponses {
        return Err(Error::message("Codex 0.154.0 requires openai_responses"));
    }
    if request.project_local.is_some() {
        return Err(Error::message(
            "Codex provider profiles are user-private and cannot be project-local",
        ));
    }
    let base = format!(
        "{}/r/{}/v1",
        request.gateway_origin.trim_end_matches('/'),
        request.root
    );
    let path = config_dir.join("llmgw.config.toml");
    let ownership_requirement = owned_provider_requirement(request, &path)?;
    let provider = json!({
        "name": "llmgw",
        "base_url": base,
        "wire_api": "responses",
        "requires_openai_auth": false,
        "supports_websockets": false,
        "http_headers": {"X-LLMGW-Token": request.local_data_token}
    });
    let edits = vec![
        set_public(&["model"], json!(request.model))?,
        set_public(&["model_provider"], json!("llmgw"))?,
        Edit::Set {
            key: key(&["model_providers", "llmgw"])?,
            value: EditValue::LocalDataTokenObject(provider),
        },
    ];
    Ok((
        vec![patch(path, Format::Toml, edits, Vec::new())?],
        base,
        Scope::UserPrivate,
        vec![
            "use `codex --profile llmgw`; the global config and MCP headers are not changed".into(),
        ],
        match ownership_requirement {
            Some(requirement) => vec![PlanCheck::OwnedProvider {
                journal_path: absolute(&request.journal_path)?,
                requirement,
            }],
            None => Vec::new(),
        },
    ))
}

fn owned_provider_requirement(
    request: &ProfileRequest,
    path: &Path,
) -> Result<Option<crate::config_patch::OwnedKeyRequirement>, Error> {
    let (_, bytes) = snapshot(path)?;
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let value = crate::config_patch::document::parse_document(Format::Toml, &bytes)?;
    if value.pointer("/model_providers/llmgw").is_none() {
        return Ok(None);
    }
    let provider_key = key(&["model_providers", "llmgw"])?;
    match crate::config_patch::journal::owned_key_current_matches(
        &request.journal_path,
        path,
        Format::Toml,
        &provider_key,
    )? {
        Some(true) => Ok(Some(crate::config_patch::OwnedKeyRequirement {
            resource_path: path.to_owned(),
            format: Format::Toml,
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
