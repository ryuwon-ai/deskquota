//! Selective single-pass policy inspection. Content scanning visits only protocol
//! input locations; tool arguments, schemas, and unknown top-level data stay opaque.
use super::{DataRoute, ProtocolError, reject};
use crate::admission::quota::RequestCost;
use crate::config::{Config, Endpoint, Limit};
use axum::http::StatusCode;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;

pub fn inspect_body(
    config: &Config,
    route: &DataRoute,
    body: &bytes::Bytes,
) -> Result<RequestCost, ProtocolError> {
    if route.endpoint == Endpoint::Models {
        return Ok(RequestCost::Metadata);
    }
    let text =
        std::str::from_utf8(body).map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_json"))?;
    let mut de = serde_json::Deserializer::from_str(text);
    let inspected = (&mut de)
        .deserialize_any(InspectionVisitor(route.endpoint))
        .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_json"))?;
    de.end()
        .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_json"))?;
    let Some(i) = inspected else {
        return Err(reject(StatusCode::BAD_REQUEST, "json_object_required"));
    };
    let error = |code| reject(StatusCode::BAD_REQUEST, code);
    if i.duplicate {
        return Err(error("duplicate_inspected_field"));
    }
    if route.endpoint == Endpoint::Responses && i.background {
        return Err(error("unsupported_background"));
    }
    let model = i.model.as_deref().ok_or_else(|| error("model_required"))?;
    if !config.roots[route.root_index]
        .models
        .iter()
        .any(|allowed| allowed == model)
    {
        return Err(error("model_not_allowed"));
    }
    if route.endpoint == Endpoint::CountTokens {
        return Ok(RequestCost::Metadata);
    }
    let Limit::Known(limit) = config.quota.tpm else {
        return Ok(RequestCost::Metadata);
    };
    if i.multimodal {
        return Err(error("unsupported_multimodal_estimate"));
    }
    if i.n.is_some_and(|n| n != Some(1)) {
        return Err(error("unsupported_generation_count"));
    }
    if i.caps.iter().any(|cap| matches!(cap, Some(None))) {
        return Err(error("invalid_output_bound"));
    }
    let cap = match route.endpoint {
        Endpoint::ChatCompletions => {
            if i.caps[2].is_some() {
                return Err(error("unsupported_output_bound_field"));
            }
            if i.caps[0].is_some() && i.caps[1].is_some() {
                return Err(error("ambiguous_output_bound"));
            }
            i.caps[1].or(i.caps[0]).flatten()
        }
        Endpoint::Responses => {
            if i.caps[0].is_some() || i.caps[1].is_some() {
                return Err(error("unsupported_output_bound_field"));
            }
            i.caps[2].flatten()
        }
        Endpoint::Messages => {
            if i.caps[1].is_some() || i.caps[2].is_some() {
                return Err(error("unsupported_output_bound_field"));
            }
            i.caps[0].flatten()
        }
        Endpoint::Models | Endpoint::CountTokens => unreachable!(),
    };
    let output = cap
        .or_else(|| {
            config
                .models
                .iter()
                .find(|m| m.id == model)
                .and_then(|m| m.max_output_tokens.map(|n| n.get()))
        })
        .ok_or_else(|| error("output_bound_required"))?;
    let input = body.len() as u64;
    if input
        .checked_add(output)
        .is_none_or(|total| total > limit.get())
    {
        return Err(error("estimate_exceeds_budget"));
    }
    Ok(RequestCost::estimated(input, output))
}
use serde::Deserializer;
#[derive(Default)]
struct Inspection {
    model: Option<String>,
    background: bool,
    duplicate: bool,
    caps: [Option<Option<u64>>; 3],
    multimodal: bool,
    n: Option<Option<u64>>,
}
struct InspectionVisitor(Endpoint);
impl<'de> Visitor<'de> for InspectionVisitor {
    type Value = Option<Inspection>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut i = Inspection::default();
        let mut seen = 0u16;
        while let Some(field) = map.next_key::<String>()? {
            let bit = match field.as_str() {
                "model" => 1,
                "background" => 2,
                "max_tokens" => 4,
                "max_completion_tokens" => 8,
                "max_output_tokens" => 16,
                "messages" if self.0 != Endpoint::Responses => 32,
                "input" if self.0 == Endpoint::Responses => 64,
                "modalities" if self.0 == Endpoint::ChatCompletions => 128,
                "n" if self.0 == Endpoint::ChatCompletions => 256,
                _ => 0,
            };
            if bit != 0 && seen & bit != 0 {
                i.duplicate = true;
                map.next_value::<IgnoredAny>()?;
                continue;
            }
            seen |= bit;
            match field.as_str() {
                "model" => {
                    let value = map.next_value::<serde_json::Value>()?;
                    i.model = value.as_str().map(str::to_owned);
                }
                "background" => {
                    let value = map.next_value::<serde_json::Value>()?;
                    i.background = value.as_bool() == Some(true);
                }
                "max_tokens" | "max_completion_tokens" | "max_output_tokens" => {
                    let index = match bit {
                        4 => 0,
                        8 => 1,
                        _ => 2,
                    };
                    i.caps[index] = Some(map.next_value_seed(Number)?);
                }
                "n" if bit == 256 => i.n = Some(map.next_value_seed(Number)?),
                "messages" if bit == 32 => i.multimodal |= map.next_value_seed(Scan::Messages)?,
                "input" if bit == 64 => i.multimodal |= map.next_value_seed(Scan::Input)?,
                "modalities" if bit == 128 => {
                    i.multimodal |= map.next_value_seed(Scan::Modalities)?
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Some(i))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }
    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}
struct Number;
impl<'de> DeserializeSeed<'de> for Number {
    type Value = Option<u64>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Number {
    type Value = Option<u64>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("positive integer")
    }
    fn visit_u64<E>(self, n: u64) -> Result<Self::Value, E> {
        Ok((n > 0).then_some(n))
    }
    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
        while s.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
        while m.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(None)
    }
}
#[derive(Clone, Copy)]
enum Scan {
    Messages,
    Message,
    Input,
    Item,
    Content,
    Block,
    Modalities,
    Kind,
}
impl<'de> DeserializeSeed<'de> for Scan {
    type Value = bool;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Scan {
    type Value = bool;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("protocol input")
    }
    fn visit_str<E>(self, s: &str) -> Result<bool, E> {
        Ok(match self {
            Self::Kind => !matches!(
                s,
                "text"
                    | "input_text"
                    | "output_text"
                    | "message"
                    | "function_call"
                    | "function_call_output"
                    | "tool_use"
                    | "tool_result"
                    | "thinking"
                    | "redacted_thinking"
                    | "refusal"
            ),
            Self::Modalities => s != "text",
            Self::Messages | Self::Message | Self::Block | Self::Item => true,
            _ => false,
        })
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<bool, A::Error> {
        let child = match self {
            Self::Messages => Self::Message,
            Self::Input => Self::Item,
            Self::Content => Self::Block,
            Self::Modalities => Self::Modalities,
            _ => {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                return Ok(true);
            }
        };
        let mut unsupported = false;
        while let Some(value) = seq.next_element_seed(child)? {
            unsupported |= value;
        }
        Ok(unsupported)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<bool, A::Error> {
        if !matches!(self, Self::Message | Self::Item | Self::Block) {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            return Ok(true);
        }
        let mut unsupported = false;
        let mut seen = 0u8;
        while let Some(key) = map.next_key::<String>()? {
            let bit = match key.as_str() {
                "type" => 1,
                "content" => 2,
                "audio" => 4,
                "output" if matches!(self, Self::Item) => 8,
                _ => 0,
            };
            if bit != 0 && seen & bit != 0 {
                unsupported = true;
            }
            seen |= bit;
            match key.as_str() {
                "type" => unsupported |= map.next_value_seed(Self::Kind)?,
                "content" => unsupported |= map.next_value_seed(Self::Content)?,
                "output" if matches!(self, Self::Item) => {
                    unsupported |= map.next_value_seed(Self::Content)?
                }
                "audio" => {
                    unsupported = true;
                    map.next_value::<IgnoredAny>()?;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if matches!(self, Self::Block) && seen & 1 == 0 {
            unsupported = true;
        }
        Ok(unsupported)
    }
    fn visit_unit<E>(self) -> Result<bool, E> {
        Ok(!matches!(self, Self::Content))
    }
    fn visit_bool<E>(self, _: bool) -> Result<bool, E> {
        Ok(true)
    }
    fn visit_i64<E>(self, _: i64) -> Result<bool, E> {
        Ok(true)
    }
    fn visit_u64<E>(self, _: u64) -> Result<bool, E> {
        Ok(true)
    }
    fn visit_f64<E>(self, _: f64) -> Result<bool, E> {
        Ok(true)
    }
}
