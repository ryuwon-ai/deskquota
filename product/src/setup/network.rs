//! Explicit bounded model-listing and one-call inference doctor operations.
use super::*;

#[derive(Debug)]
pub struct ListRequest {
    pub url: url::Url,
    pub header: Option<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelListing {
    Available(Vec<String>),
    Unavailable { status: Option<u16>, reason: String },
}

pub fn list_models<F>(base: &str, auth: &Auth, send: F) -> Result<ModelListing, Error>
where
    F: FnOnce(ListRequest) -> Result<(u16, Vec<u8>), Error>,
{
    let base = validate::api_base(base)?;
    let mut url = base.clone();
    url.set_path(&format!("{}/models", base.path().trim_end_matches('/')));
    let header = validate::auth_header(auth)?;
    let (status, body) = send(ListRequest { url, header })?;
    parse_listing_response(status, body)
}

fn parse_listing_response(status: u16, body: Vec<u8>) -> Result<ModelListing, Error> {
    if status != 200 {
        return Ok(ModelListing::Unavailable {
            status: Some(status),
            reason: format!("model listing returned HTTP {status}; enter a model ID manually"),
        });
    }
    if body.len() > 65_536 {
        return Ok(ModelListing::Unavailable {
            status: Some(status),
            reason: "model listing exceeded 65536 bytes".into(),
        });
    }
    #[derive(serde::Deserialize)]
    struct Response {
        data: Vec<Item>,
    }
    #[derive(serde::Deserialize)]
    struct Item {
        id: String,
    }
    let parsed: Response = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(ModelListing::Unavailable {
                status: Some(status),
                reason: "model listing response was not the supported JSON shape".into(),
            });
        }
    };
    let mut ids: Vec<_> = parsed
        .data
        .into_iter()
        .map(|item| item.id)
        .filter(|id| !id.is_empty())
        .take(256)
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ModelListing::Available(ids))
}

pub async fn fetch_models(upstream: &Upstream) -> Result<ModelListing, Error> {
    let client =
        crate::transport::upstream::build_client(std::time::Duration::from_secs(10), upstream)
            .map_err(|error| Error::message(error.to_string()))?;
    let mut url = upstream.api_base.clone();
    url.set_path(&format!(
        "{}/models",
        upstream.api_base.path().trim_end_matches('/')
    ));
    let mut request = client.get(url);
    let Some((header, value)) = validate::auth_header(&upstream.auth).map_err(|error| {
        Error::message(format!(
            "model listing unavailable: {error}; enter a model ID manually"
        ))
    })?
    else {
        let mut response = request.send().await.map_err(|_| {
            Error::message("model listing network request failed; enter a model ID manually")
        })?;
        return bounded_listing_response(&mut response).await;
    };
    request = request.header(header, value);
    let mut response = request.send().await.map_err(|_| {
        Error::message("model listing network request failed; enter a model ID manually")
    })?;
    bounded_listing_response(&mut response).await
}

async fn bounded_listing_response(response: &mut reqwest::Response) -> Result<ModelListing, Error> {
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::message("model listing response read failed"))?
    {
        if body.len() + chunk.len() > 65_536 {
            return Ok(ModelListing::Unavailable {
                status: Some(status),
                reason: "model listing exceeded 65536 bytes".into(),
            });
        }
        body.extend_from_slice(&chunk);
    }
    parse_listing_response(status, body)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferenceResult {
    pub status: u16,
}

pub async fn smoke_inference(
    config: &Config,
    model: &str,
    max_output_tokens: u64,
) -> Result<InferenceResult, Error> {
    if max_output_tokens == 0 {
        return Err(Error::message("inference output bound must be nonzero"));
    }
    if !config
        .models
        .iter()
        .any(|configured| configured.id == model)
    {
        return Err(Error::message("inference model is not configured"));
    }
    let endpoint = config
        .roots
        .iter()
        .filter(|root| root.models.iter().any(|configured| configured == model))
        .flat_map(|root| root.endpoints.iter().copied())
        .find(|endpoint| {
            matches!(
                endpoint,
                Endpoint::Responses | Endpoint::Messages | Endpoint::ChatCompletions
            )
        })
        .ok_or_else(|| Error::message("configured model has no generation endpoint"))?;
    let client = crate::transport::upstream::build_client(
        std::time::Duration::from_secs(15),
        &config.upstream,
    )
    .map_err(|error| Error::message(error.to_string()))?;
    let mut url = config.upstream.api_base.clone();
    url.set_path(&format!(
        "{}/{}",
        config.upstream.api_base.path().trim_end_matches('/'),
        endpoint.path()
    ));
    let body = match endpoint {
        Endpoint::Responses => {
            serde_json::json!({"model":model,"max_output_tokens":max_output_tokens,"input":"Reply with OK."})
        }
        Endpoint::Messages => {
            serde_json::json!({"model":model,"max_tokens":max_output_tokens,"messages":[{"role":"user","content":"Reply with OK."}]})
        }
        Endpoint::ChatCompletions => {
            serde_json::json!({"model":model,"max_tokens":max_output_tokens,"messages":[{"role":"user","content":"Reply with OK."}]})
        }
        Endpoint::CountTokens | Endpoint::Models => unreachable!(),
    };
    let mut request = client
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string());
    if let Some((header, value)) = validate::auth_header(&config.upstream.auth)? {
        request = request.header(header, value);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| Error::message("inference network request failed"))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::message("inference response read failed"))?
    {
        let size = body
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| Error::message("inference response size overflow"))?;
        if size > 65_536 {
            return Err(Error::message("inference response exceeded 65536 bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    if !(200..300).contains(&status) {
        return Err(Error::message(format!("inference returned HTTP {status}")));
    }
    validate_generation_response(endpoint, &body)?;
    Ok(InferenceResult { status })
}

fn validate_generation_response(endpoint: Endpoint, body: &[u8]) -> Result<(), Error> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| Error::message("inference response lacked valid JSON generation evidence"))?;
    if value.get("error").is_some_and(|error| !error.is_null())
        || matches!(
            value.get("status").and_then(|status| status.as_str()),
            Some("failed" | "cancelled" | "incomplete")
        )
    {
        return Err(Error::message("inference provider reported failure"));
    }
    let nonempty =
        |value: &serde_json::Value| value.as_str().is_some_and(|text| !text.trim().is_empty());
    let generated = match endpoint {
        Endpoint::Responses => {
            value.get("status").and_then(|status| status.as_str()) == Some("completed")
                && value
                    .get("output")
                    .and_then(|output| output.as_array())
                    .is_some_and(|items| {
                        items.iter().any(|item| {
                            item.get("content")
                                .and_then(|content| content.as_array())
                                .is_some_and(|content| {
                                    content.iter().any(|part| {
                                        part.get("type").and_then(|kind| kind.as_str())
                                            == Some("output_text")
                                            && part.get("text").is_some_and(nonempty)
                                    })
                                })
                        })
                    })
        }
        Endpoint::Messages => {
            value.get("type").and_then(|kind| kind.as_str()) == Some("message")
                && value
                    .get("content")
                    .and_then(|content| content.as_array())
                    .is_some_and(|content| {
                        content.iter().any(|part| {
                            part.get("type").and_then(|kind| kind.as_str()) == Some("text")
                                && part.get("text").is_some_and(nonempty)
                        })
                    })
        }
        Endpoint::ChatCompletions => value
            .get("choices")
            .and_then(|choices| choices.as_array())
            .is_some_and(|choices| {
                choices
                    .iter()
                    .any(|choice| choice.pointer("/message/content").is_some_and(nonempty))
            }),
        Endpoint::CountTokens | Endpoint::Models => false,
    };
    if !generated {
        return Err(Error::message(
            "inference response lacked protocol-specific generation evidence",
        ));
    }
    Ok(())
}
