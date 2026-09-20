//! Opt-in exact responses; no request text or credential survives outside its digest.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Response, StatusCode};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use crate::config::{CacheConfig, Endpoint};
use crate::transport::headers::stored_size;

const BUDGET_BYTES: usize = 4 * 1024 * 1024;
const ENTRY_BYTES: usize = 256 * 1024;
const MAX_ENTRIES: usize = 128;
const REQUEST_BYTES: usize = 32 * 1024;
type Key = [u8; 32];

#[derive(Clone)]
pub(crate) struct ExactCache(Arc<Inner>);
struct Inner {
    config: CacheConfig,
    budget: Arc<Semaphore>,
    state: Mutex<State>,
    filled: Notify,
}
#[derive(Default)]
struct State {
    entries: HashMap<Key, Arc<Entry>>,
    flights: HashSet<Key>,
    waiters: usize,
    coalesced: u64,
    coordination_bypasses: u64,
    considered: u64,
    bypasses: Bypasses,
    not_stored: NotStoredCounts,
    hits: u64,
    misses: u64,
    stores: u64,
    evictions: u64,
    budget_bypasses: u64,
}

#[derive(Clone, Copy, Default, Serialize)]
struct Bypasses {
    request_cache_control: u64,
    size: u64,
    endpoint: u64,
    tools_state: u64,
    history: u64,
    unsupported_shape: u64,
}

#[derive(Clone, Copy, Default, Serialize)]
struct NotStoredCounts {
    status: u64,
    response_cache_control: u64,
    unsafe_headers: u64,
    encoding: u64,
    representation: u64,
    size: u64,
    incomplete_or_unsafe: u64,
}

enum NotStored {
    Status,
    ResponseCacheControl,
    UnsafeHeaders,
    Encoding,
    Representation,
    Size,
    IncompleteOrUnsafe,
}

#[derive(Debug, PartialEq, Eq)]
enum Bypass {
    RequestCacheControl,
    Size,
    Endpoint,
    ToolsState,
    History,
    UnsupportedShape,
}
struct Entry {
    body: Box<[u8]>,
    headers: HeaderMap,
    inserted: Instant,
    _permit: OwnedSemaphorePermit,
}
struct Replay(Arc<Entry>);
impl AsRef<[u8]> for Replay {
    fn as_ref(&self) -> &[u8] {
        &self.0.body
    }
}

#[derive(Serialize)]
pub(crate) struct Snapshot {
    enabled: bool,
    considered: u64,
    bypasses: Bypasses,
    not_stored: NotStoredCounts,
    hits: u64,
    misses: u64,
    stores: u64,
    evictions: u64,
    budget_bypasses: u64,
    entries: usize,
    inflight_keys: usize,
    waiters: usize,
    coalesced: u64,
    coordination_bypasses: u64,
    retained_bytes: usize,
    budget_bytes: usize,
}

pub(crate) struct Pending {
    cache: ExactCache,
    key: Key,
    outcome: LookupOutcome,
    flight: Flight,
    waited: bool,
    // A captured body must either commit or record one reason, including early task exits.
    capture_pending: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flight {
    Unclaimed,
    Waiting,
    Owner,
    Bypass,
}
enum LookupOutcome {
    Uncounted,
    Miss,
    Hit,
}
pub(crate) struct Capture {
    pending: Pending,
    body: Vec<u8>,
    headers: HeaderMap,
    permit: OwnedSemaphorePermit,
    limit: usize,
}

impl ExactCache {
    pub(crate) fn new(config: CacheConfig) -> Self {
        Self(Arc::new(Inner {
            config,
            budget: Arc::new(Semaphore::new(BUDGET_BYTES)),
            state: Mutex::new(State::default()),
            filled: Notify::new(),
        }))
    }

    pub(crate) fn request(
        &self,
        root: usize,
        url: &url::Url,
        headers: &HeaderMap,
        endpoint: Endpoint,
        body: &[u8],
        forbids_reuse: bool,
    ) -> Option<Pending> {
        let bypass = bypass_reason(endpoint, body, self.0.config.max_history, forbids_reuse);
        let mut state = self.0.state.lock().expect("cache mutex");
        state.considered = state.considered.saturating_add(1);
        if let Some(reason) = bypass {
            let count = match reason {
                Bypass::RequestCacheControl => &mut state.bypasses.request_cache_control,
                Bypass::Size => &mut state.bypasses.size,
                Bypass::Endpoint => &mut state.bypasses.endpoint,
                Bypass::ToolsState => &mut state.bypasses.tools_state,
                Bypass::History => &mut state.bypasses.history,
                Bypass::UnsupportedShape => &mut state.bypasses.unsupported_shape,
            };
            *count = count.saturating_add(1);
            return None;
        }
        drop(state);
        let mut hash = Sha256::new();
        fn frame(hash: &mut Sha256, bytes: &[u8]) {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        frame(&mut hash, &(root as u64).to_be_bytes());
        frame(&mut hash, url.as_str().as_bytes());
        let mut names: Vec<_> = headers
            .keys()
            .filter(|name| name.as_str() != "x-stainless-retry-count")
            .collect();
        names.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        frame(&mut hash, &(names.len() as u64).to_be_bytes());
        for name in names {
            frame(&mut hash, name.as_str().as_bytes());
            frame(
                &mut hash,
                &(headers.get_all(name).iter().count() as u64).to_be_bytes(),
            );
            for value in headers.get_all(name) {
                frame(&mut hash, value.as_bytes());
            }
        }
        frame(&mut hash, body);
        Some(Pending {
            cache: self.clone(),
            key: hash.finalize().into(),
            outcome: LookupOutcome::Uncounted,
            flight: Flight::Unclaimed,
            waited: false,
            capture_pending: false,
        })
    }

    pub(crate) fn snapshot(cache: Option<&Self>) -> Snapshot {
        let mut result = Snapshot {
            enabled: cache.is_some(),
            considered: 0,
            bypasses: Bypasses::default(),
            not_stored: NotStoredCounts::default(),
            hits: 0,
            misses: 0,
            stores: 0,
            evictions: 0,
            budget_bypasses: 0,
            entries: 0,
            inflight_keys: 0,
            waiters: 0,
            coalesced: 0,
            coordination_bypasses: 0,
            retained_bytes: 0,
            budget_bytes: BUDGET_BYTES,
        };
        if let Some(cache) = cache {
            let state = cache.0.state.lock().expect("cache mutex");
            result.considered = state.considered;
            result.bypasses = state.bypasses;
            result.not_stored = state.not_stored;
            result.hits = state.hits;
            result.misses = state.misses;
            result.stores = state.stores;
            result.evictions = state.evictions;
            result.budget_bypasses = state.budget_bypasses;
            result.entries = state.entries.len();
            result.inflight_keys = state.flights.len();
            result.waiters = state.waiters;
            result.coalesced = state.coalesced;
            result.coordination_bypasses = state.coordination_bypasses;
            result.retained_bytes = BUDGET_BYTES - cache.0.budget.available_permits();
        }
        result
    }
}

impl Pending {
    pub(crate) fn lookup(&mut self) -> Option<Response<Body>> {
        let cache = self.cache.clone();
        let mut state = cache.0.state.lock().expect("cache mutex");
        self.lookup_in(&mut state)
    }

    fn lookup_in(&mut self, state: &mut State) -> Option<Response<Body>> {
        state.entries.retain(|_, entry| {
            entry.inserted.elapsed() < Duration::from_secs(self.cache.0.config.ttl_secs)
        });
        let entry = state.entries.get(&self.key).cloned();
        match self.outcome {
            LookupOutcome::Uncounted => {
                self.outcome = if entry.is_some() {
                    state.hits += 1;
                    LookupOutcome::Hit
                } else {
                    state.misses += 1;
                    LookupOutcome::Miss
                };
            }
            LookupOutcome::Miss if entry.is_some() => {
                state.misses -= 1;
                state.hits += 1;
                self.outcome = LookupOutcome::Hit;
                if self.waited {
                    state.coalesced = state.coalesced.saturating_add(1);
                }
            }
            _ => {}
        }
        if let Some(entry) = entry {
            self.leave_waiters(state);
            let mut response = Response::new(Body::from(Bytes::from_owner(Replay(entry.clone()))));
            *response.headers_mut() = entry.headers.clone();
            Some(response)
        } else {
            None
        }
    }

    fn leave_waiters(&mut self, state: &mut State) {
        if self.flight == Flight::Waiting {
            state.waiters -= 1;
            self.flight = Flight::Unclaimed;
        }
    }

    /// Return a replay, or elect this request to perform the next possible fill.
    pub(crate) async fn wait_for_turn(&mut self) -> Option<Response<Body>> {
        let cache = self.cache.clone();
        loop {
            let notified = cache.0.filled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = cache.0.state.lock().expect("cache mutex");
                if let Some(hit) = self.lookup_in(&mut state) {
                    return Some(hit);
                }
                if matches!(self.flight, Flight::Owner | Flight::Bypass) {
                    return None;
                }
                if !state.flights.contains(&self.key) {
                    self.leave_waiters(&mut state);
                    if state.flights.len() < MAX_ENTRIES {
                        state.flights.insert(self.key);
                        self.flight = Flight::Owner;
                    } else {
                        // ponytail: bounded coordination fails open at 128 keys; admission still limits wire work.
                        state.coordination_bypasses = state.coordination_bypasses.saturating_add(1);
                        self.flight = Flight::Bypass;
                    }
                    return None;
                }
                if self.flight != Flight::Waiting {
                    state.waiters += 1;
                    self.flight = Flight::Waiting;
                    self.waited = true;
                }
            }
            notified.await;
        }
    }

    pub(crate) fn capture(
        mut self,
        status: StatusCode,
        original: &HeaderMap,
        forwarded: &HeaderMap,
    ) -> Option<Capture> {
        let rejection = if status != StatusCode::OK {
            Some(NotStored::Status)
        } else if forbids_reuse(original) {
            Some(NotStored::ResponseCacheControl)
        } else if original.contains_key("set-cookie")
            || original.get_all("vary").iter().any(|value| {
                value.to_str().map_or(true, |value| {
                    value.split(',').any(|field| {
                        field.trim() == "*"
                            || field.trim().eq_ignore_ascii_case("x-stainless-retry-count")
                    })
                })
            })
        {
            Some(NotStored::UnsafeHeaders)
        } else if original
            .get_all("content-encoding")
            .iter()
            .any(|value| !value.as_bytes().eq_ignore_ascii_case(b"identity"))
        {
            Some(NotStored::Encoding)
        } else if original.get_all("content-length").iter().any(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .is_none_or(|value| value > ENTRY_BYTES)
        }) {
            Some(NotStored::Size)
        } else {
            None
        };
        if let Some(reason) = rejection {
            self.record_not_stored(reason);
            return None;
        }
        let names = ["content-type", "content-encoding", "content-language"];
        let mut types = forwarded.get_all("content-type").iter();
        let media = types
            .next()
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        if types.next().is_some()
            || !media.is_some_and(|media| {
                media.eq_ignore_ascii_case("application/json")
                    || media.eq_ignore_ascii_case("text/event-stream")
            })
        {
            self.record_not_stored(NotStored::Representation);
            return None;
        }
        let limit = names
            .iter()
            .try_fold(2usize, |total, name| {
                forwarded
                    .get_all(*name)
                    .iter()
                    .try_fold(total, |total, value| {
                        total.checked_add(name.len() + value.as_bytes().len() + 4)
                    })
            })
            .and_then(|header_bytes| ENTRY_BYTES.checked_sub(header_bytes));
        let Some(limit) = limit else {
            self.record_not_stored(NotStored::Size);
            return None;
        };
        let permit = {
            let mut state = self.cache.0.state.lock().expect("cache mutex");
            loop {
                if let Ok(permit) = self
                    .cache
                    .0
                    .budget
                    .clone()
                    .try_acquire_many_owned(ENTRY_BYTES as u32)
                {
                    break permit;
                }
                if !evict(&mut state) {
                    state.budget_bypasses += 1;
                    return None;
                }
            }
        };
        // Reserve before copying; values must not retain the original HTTP head buffer.
        // Only representation headers are replayed; request IDs and quota metadata are stale.
        let mut headers = HeaderMap::new();
        for name in names {
            for value in forwarded.get_all(name) {
                headers.append(
                    name,
                    axum::http::HeaderValue::from_bytes(value.as_bytes())
                        .expect("existing header value"),
                );
            }
        }
        self.capture_pending = true;
        Some(Capture {
            pending: self,
            body: Vec::with_capacity(limit),
            headers,
            permit,
            limit,
        })
    }

    pub(crate) fn reject_status(mut self) {
        self.record_not_stored(NotStored::Status);
    }

    fn record_not_stored(&mut self, reason: NotStored) {
        self.capture_pending = false;
        let mut state = self.cache.0.state.lock().expect("cache mutex");
        let counts = &mut state.not_stored;
        let count = match reason {
            NotStored::Status => &mut counts.status,
            NotStored::ResponseCacheControl => &mut counts.response_cache_control,
            NotStored::UnsafeHeaders => &mut counts.unsafe_headers,
            NotStored::Encoding => &mut counts.encoding,
            NotStored::Representation => &mut counts.representation,
            NotStored::Size => &mut counts.size,
            NotStored::IncompleteOrUnsafe => &mut counts.incomplete_or_unsafe,
        };
        *count = count.saturating_add(1);
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.capture_pending {
            self.record_not_stored(NotStored::IncompleteOrUnsafe);
        }
        if !matches!(self.flight, Flight::Waiting | Flight::Owner) {
            return;
        }
        let mut state = self.cache.0.state.lock().expect("cache mutex");
        match self.flight {
            Flight::Waiting => state.waiters -= 1,
            Flight::Owner => {
                state.flights.remove(&self.key);
                drop(state);
                // A failed, cancelled, or non-cacheable owner must release the next real request.
                self.cache.0.filled.notify_waiters();
            }
            Flight::Unclaimed | Flight::Bypass => {}
        }
    }
}

fn evict(state: &mut State) -> bool {
    // ponytail: scan at most 128 entries; replace only if profiling justifies an LRU list.
    let oldest = state
        .entries
        .iter()
        .min_by_key(|(_, entry)| entry.inserted)
        .map(|(key, _)| *key);
    if let Some(key) = oldest {
        state.entries.remove(&key);
        state.evictions += 1;
        true
    } else {
        false
    }
}

impl Capture {
    pub(crate) fn observe(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > self.limit - self.body.len() {
            if self.pending.capture_pending {
                self.pending.record_not_stored(NotStored::Size);
            }
            return false;
        }
        // Copy from upstream before delivery; never retain the delivery semaphore owner.
        self.body.extend_from_slice(bytes);
        true
    }
    pub(crate) fn commit(mut self, complete: bool) {
        if !complete || !self.pending.capture_pending {
            return;
        }
        self.pending.capture_pending = false;
        let body = self.body.into_boxed_slice();
        let retained = body.len() + stored_size(&self.headers);
        drop(self.permit.split(ENTRY_BYTES - retained));
        let entry = Arc::new(Entry {
            body,
            headers: self.headers,
            inserted: Instant::now(),
            _permit: self.permit,
        });
        let mut state = self.pending.cache.0.state.lock().expect("cache mutex");
        if state.entries.len() >= MAX_ENTRIES && !state.entries.contains_key(&self.pending.key) {
            evict(&mut state);
        }
        state.entries.insert(self.pending.key, entry);
        state.stores += 1;
        if self.pending.flight == Flight::Owner {
            state.flights.remove(&self.pending.key);
            self.pending.flight = Flight::Unclaimed;
        }
        drop(state);
        // ponytail: wake the bounded waiter set; use per-key notifications only if profiling warrants it.
        self.pending.cache.0.filled.notify_waiters();
    }
}

pub(crate) fn forbids_reuse(headers: &HeaderMap) -> bool {
    headers
        .get_all("cache-control")
        .iter()
        .chain(headers.get_all("pragma").iter())
        .any(|value| {
            value.to_str().map_or(true, |value| {
                value.split(',').any(|directive| {
                    let name = directive.split('=').next().unwrap_or("").trim();
                    name.eq_ignore_ascii_case("no-store")
                        || name.eq_ignore_ascii_case("no-cache")
                        || directive.trim().eq_ignore_ascii_case("max-age=0")
                })
            })
        })
}

fn only(value: &Value, fields: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}
fn text(value: &Value, block_types: &[&str]) -> bool {
    value.is_string()
        || value.as_array().is_some_and(|blocks| {
            !blocks.is_empty()
                && blocks.iter().all(|block| {
                    only(block, &["type", "text"])
                        && block
                            .get("type")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| block_types.contains(&kind))
                        && block.get("text").is_some_and(Value::is_string)
                })
        })
}
fn messages(value: &Value, max_history: usize, types: &[&str]) -> bool {
    value.as_array().is_some_and(|messages| {
        !messages.is_empty()
            && messages.len() <= max_history
            && messages.iter().all(|message| {
                only(message, &["role", "content", "type"])
                    && message.get("type").is_none_or(|kind| kind == "message")
                    && message
                        .get("role")
                        .and_then(Value::as_str)
                        .is_some_and(|role| {
                            matches!(role, "system" | "developer" | "user" | "assistant")
                        })
                    && message
                        .get("content")
                        .is_some_and(|value| text(value, types))
            })
    })
}
fn bypass_reason(
    endpoint: Endpoint,
    body: &[u8],
    max_history: usize,
    forbids_reuse: bool,
) -> Option<Bypass> {
    if forbids_reuse {
        return Some(Bypass::RequestCacheControl);
    }
    if body.len() > REQUEST_BYTES {
        return Some(Bypass::Size);
    }
    if matches!(endpoint, Endpoint::Models | Endpoint::CountTokens) {
        return Some(Bypass::Endpoint);
    }
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Some(Bypass::UnsupportedShape);
    };
    let input = value.get(if endpoint == Endpoint::Responses {
        "input"
    } else {
        "messages"
    });
    if [
        "tools",
        "tool_choice",
        "functions",
        "function_call",
        "parallel_tool_calls",
        "previous_response_id",
        "conversation",
    ]
    .iter()
    .any(|field| value.get(field).is_some())
        || (endpoint == Endpoint::Responses
            && value.get("store").and_then(Value::as_bool) != Some(false))
        || input.and_then(Value::as_array).is_some_and(|items| {
            items.iter().any(|item| {
                matches!(
                    item.get("role").and_then(Value::as_str),
                    Some("tool" | "function")
                ) || ["tool_calls", "function_call", "tool_call_id"]
                    .iter()
                    .any(|field| item.get(field).is_some())
                    || matches!(
                        item.get("type").and_then(Value::as_str),
                        Some("function_call" | "function_call_output" | "tool_use" | "tool_result")
                    )
                    || item
                        .get("content")
                        .and_then(Value::as_array)
                        .is_some_and(|blocks| {
                            blocks.iter().any(|block| {
                                matches!(
                                    block.get("type").and_then(Value::as_str),
                                    Some("tool_use" | "tool_result")
                                )
                            })
                        })
            })
        })
    {
        return Some(Bypass::ToolsState);
    }
    if input
        .and_then(Value::as_array)
        .is_some_and(|items| items.len() > max_history)
    {
        return Some(Bypass::History);
    }
    (!eligible(endpoint, &value, max_history)).then_some(Bypass::UnsupportedShape)
}

fn eligible(endpoint: Endpoint, value: &Value, max_history: usize) -> bool {
    let fields: &[&str] = match endpoint {
        Endpoint::ChatCompletions => &[
            "model",
            "messages",
            "temperature",
            "top_p",
            "max_tokens",
            "max_completion_tokens",
            "stream",
            "stream_options",
            "stop",
            "seed",
            "frequency_penalty",
            "presence_penalty",
            "response_format",
            "n",
        ],
        Endpoint::Messages => &[
            "model",
            "messages",
            "system",
            "temperature",
            "top_p",
            "top_k",
            "max_tokens",
            "stream",
            "stop_sequences",
        ],
        Endpoint::Responses => &[
            "model",
            "input",
            "instructions",
            "temperature",
            "top_p",
            "max_output_tokens",
            "stream",
            "store",
            "text",
        ],
        _ => return false,
    };
    if !only(value, fields)
        || value
            .get("stream")
            .is_some_and(|stream| !stream.is_boolean())
    {
        return false;
    }
    if value.get("stream_options").is_some_and(|options| {
        !only(options, &["include_usage"])
            || options
                .get("include_usage")
                .is_some_and(|usage| !usage.is_boolean())
    }) {
        return false;
    }
    match endpoint {
        Endpoint::ChatCompletions => {
            value.get("n").is_none_or(|n| n.as_u64() == Some(1))
                && value
                    .get("messages")
                    .is_some_and(|value| messages(value, max_history, &["text"]))
        }
        Endpoint::Messages => {
            value
                .get("system")
                .is_none_or(|value| text(value, &["text"]))
                && value
                    .get("messages")
                    .is_some_and(|value| messages(value, max_history, &["text"]))
        }
        Endpoint::Responses => {
            value.get("store").and_then(Value::as_bool) == Some(false)
                && value.get("instructions").is_none_or(Value::is_string)
                && value.get("input").is_some_and(|value| {
                    value.is_string()
                        || messages(value, max_history, &["input_text", "output_text"])
                })
        }
        _ => false,
    }
}

pub(crate) fn complete_json(endpoint: Endpoint, value: &Value) -> bool {
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return false;
    }
    match endpoint {
        Endpoint::ChatCompletions => {
            value
                .get("choices")
                .and_then(Value::as_array)
                .is_some_and(|choices| {
                    choices.len() == 1
                        && choices[0]
                            .get("finish_reason")
                            .is_some_and(|reason| reason == "stop")
                        && choices[0].get("message").is_some_and(|message| {
                            only(message, &["role", "content", "refusal", "annotations"])
                                && message.get("refusal").is_none_or(Value::is_null)
                                && message
                                    .get("annotations")
                                    .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
                                && message.get("role").is_some_and(|role| role == "assistant")
                                && message.get("content").is_some_and(Value::is_string)
                        })
                })
        }
        Endpoint::Messages => {
            value.get("type").is_some_and(|kind| kind == "message")
                && value.get("role").is_some_and(|role| role == "assistant")
                && value
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .is_some_and(|reason| matches!(reason, "end_turn" | "stop_sequence"))
                && value
                    .get("content")
                    .is_some_and(|content| text(content, &["text"]))
        }
        Endpoint::Responses => complete_response(value),
        _ => false,
    }
}
fn response_text_block(block: &Value) -> bool {
    only(block, &["type", "text", "annotations", "logprobs"])
        && block.get("type").is_some_and(|kind| kind == "output_text")
        && block.get("text").is_some_and(Value::is_string)
        && block
            .get("annotations")
            .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty))
        && block.get("logprobs").is_none_or(Value::is_array)
}

fn response_message(item: &Value, complete: bool) -> bool {
    item.get("type").is_some_and(|kind| kind == "message")
        && item.get("role").is_some_and(|role| role == "assistant")
        && item
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status == "completed" || (!complete && status == "in_progress"))
        && item
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                (!complete || !blocks.is_empty()) && blocks.iter().all(response_text_block)
            })
}

pub(crate) fn complete_response(value: &Value) -> bool {
    value
        .get("status")
        .is_some_and(|status| status == "completed")
        && value.get("error").is_none_or(Value::is_null)
        && value.get("incomplete_details").is_none_or(Value::is_null)
        && value
            .get("output")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                !items.is_empty() && items.iter().all(|item| response_message(item, true))
            })
}

/// Tiny state alongside the existing usage observer; never allocates a second SSE decoder.
pub(crate) struct StreamCompletion {
    endpoint: Endpoint,
    invalid: bool,
    stopped: bool,
    terminal: bool,
    text: bool,
    message_started: bool,
    open_block: bool,
    block_index: u64,
}
impl StreamCompletion {
    pub(crate) fn new(endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            invalid: false,
            stopped: false,
            terminal: false,
            text: false,
            message_started: false,
            open_block: false,
            block_index: 0,
        }
    }
    pub(crate) fn invalidate(&mut self) {
        self.invalid = true;
    }
    pub(crate) fn done(&mut self) {
        if self.terminal {
            self.invalid = true;
        }
        self.terminal = true;
    }
    pub(crate) fn complete(&self) -> bool {
        !self.invalid && self.stopped && self.terminal && self.text
    }
    pub(crate) fn observe(&mut self, event: Option<&str>, value: &Value) {
        if self.terminal || value.get("error").is_some_and(|error| !error.is_null()) {
            self.invalid = true;
            return;
        }
        match self.endpoint {
            Endpoint::ChatCompletions => {
                let Some(choices) = value.get("choices").and_then(Value::as_array) else {
                    self.invalid = true;
                    return;
                };
                if choices.is_empty() {
                    return;
                } // optional final usage chunk
                if choices.len() != 1 || self.stopped {
                    self.invalid = true;
                    return;
                }
                let choice = &choices[0];
                if choice
                    .get("index")
                    .is_some_and(|index| index.as_u64() != Some(0))
                {
                    self.invalid = true;
                }
                let Some(delta) = choice.get("delta") else {
                    self.invalid = true;
                    return;
                };
                if !only(delta, &["role", "content", "refusal", "annotations"])
                    || delta
                        .get("annotations")
                        .is_some_and(|value| !value.as_array().is_some_and(Vec::is_empty))
                    || delta
                        .get("refusal")
                        .is_some_and(|refusal| !refusal.is_null())
                    || delta.get("role").is_some_and(|role| role != "assistant")
                    || delta
                        .get("content")
                        .is_some_and(|content| !content.is_null() && !content.is_string())
                {
                    self.invalid = true;
                }
                self.text |= delta.get("content").is_some_and(Value::is_string);
                if let Some(reason) = choice
                    .get("finish_reason")
                    .filter(|reason| !reason.is_null())
                {
                    self.stopped = reason == "stop";
                    self.invalid |= !self.stopped;
                }
            }
            Endpoint::Messages => {
                let kind = value.get("type").and_then(Value::as_str).or(event);
                if self.stopped && kind != Some("message_stop") {
                    self.invalid = true;
                }
                match kind {
                    Some("message_start") => {
                        self.invalid |= self.message_started;
                        self.message_started = true;
                        if !value.get("message").is_some_and(|message| {
                            message.get("role").is_some_and(|role| role == "assistant")
                                && message
                                    .get("content")
                                    .and_then(Value::as_array)
                                    .is_some_and(Vec::is_empty)
                        }) {
                            self.invalid = true;
                        }
                    }
                    Some("content_block_start") => {
                        self.invalid |= !self.message_started
                            || self.open_block
                            || value.get("index").and_then(Value::as_u64) != Some(self.block_index);
                        self.open_block = true;
                        let valid = value.get("content_block").is_some_and(|block| {
                            only(block, &["type", "text"])
                                && block.get("type").is_some_and(|kind| kind == "text")
                                && block.get("text").is_some_and(Value::is_string)
                        });
                        self.text |= valid;
                        self.invalid |= !valid;
                    }
                    Some("content_block_delta") => {
                        self.invalid |= !self.message_started
                            || !self.open_block
                            || value.get("index").and_then(Value::as_u64) != Some(self.block_index);
                        let valid = value.get("delta").is_some_and(|delta| {
                            only(delta, &["type", "text"])
                                && delta.get("type").is_some_and(|kind| kind == "text_delta")
                                && delta.get("text").is_some_and(Value::is_string)
                        });
                        self.text |= valid;
                        self.invalid |= !valid;
                    }
                    Some("message_delta") => {
                        self.invalid |= !self.message_started || self.open_block;
                        self.stopped = value
                            .pointer("/delta/stop_reason")
                            .and_then(Value::as_str)
                            .is_some_and(|reason| matches!(reason, "end_turn" | "stop_sequence"));
                        self.invalid |= !self.stopped;
                    }
                    Some("message_stop") => {
                        self.invalid |= !self.message_started || !self.stopped || self.open_block;
                        self.terminal = true;
                    }
                    Some("content_block_stop") => {
                        self.invalid |= !self.message_started
                            || !self.open_block
                            || value.get("index").and_then(Value::as_u64) != Some(self.block_index);
                        self.open_block = false;
                        self.block_index = self.block_index.saturating_add(1);
                    }
                    Some("ping") => {}
                    _ => self.invalid = true,
                }
            }
            Endpoint::Responses => match value.get("type").and_then(Value::as_str) {
                Some("response.completed") => {
                    self.stopped = value.get("response").is_some_and(complete_response);
                    self.text |= self.stopped;
                    self.terminal = true;
                }
                Some("response.output_text.delta") => {
                    self.invalid |= !value.get("delta").is_some_and(Value::is_string)
                        || !value.get("logprobs").is_none_or(Value::is_array);
                }
                Some("response.output_text.done") => {
                    self.invalid |= !value.get("text").is_some_and(Value::is_string)
                        || !value.get("logprobs").is_none_or(Value::is_array);
                }
                Some("response.output_item.added") => {
                    self.invalid |= !value
                        .get("item")
                        .is_some_and(|item| response_message(item, false));
                }
                Some("response.output_item.done") => {
                    self.invalid |= !value
                        .get("item")
                        .is_some_and(|item| response_message(item, true));
                }
                Some("response.content_part.added" | "response.content_part.done") => {
                    self.invalid |= !value.get("part").is_some_and(response_text_block);
                }
                Some("response.created" | "response.in_progress") => {
                    self.invalid |= !value.get("response").is_some_and(|response| {
                        response
                            .get("status")
                            .and_then(Value::as_str)
                            .is_some_and(|status| matches!(status, "queued" | "in_progress"))
                            && response.get("error").is_none_or(Value::is_null)
                            && response
                                .get("incomplete_details")
                                .is_none_or(Value::is_null)
                            && response
                                .get("output")
                                .and_then(Value::as_array)
                                .is_some_and(|items| {
                                    items.iter().all(|item| response_message(item, false))
                                })
                    });
                }
                _ => self.invalid = true,
            },
            _ => self.invalid = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    fn pending(cache: &ExactCache, key: u16) -> Pending {
        let mut digest = [0; 32];
        digest[..2].copy_from_slice(&key.to_be_bytes());
        Pending {
            cache: cache.clone(),
            key: digest,
            outcome: LookupOutcome::Uncounted,
            flight: Flight::Unclaimed,
            waited: false,
            capture_pending: false,
        }
    }
    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers
    }
    const PREFIX: &str = "{\"choices\":[{\"finish_reason\":\"stop\",\"message\":{\"role\":\"assistant\",\"content\":\"";
    const SUFFIX: &str = "\"}}]}";
    fn store(cache: &ExactCache, key: u16, full: bool) {
        let headers = headers();
        let mut capture = pending(cache, key)
            .capture(StatusCode::OK, &headers, &headers)
            .unwrap();
        let count = if full {
            capture.limit - PREFIX.len() - SUFFIX.len()
        } else {
            1
        };
        assert!(capture.observe(PREFIX.as_bytes()));
        assert!(capture.observe(&vec![b'x'; count]));
        assert!(capture.observe(SUFFIX.as_bytes()));
        capture.commit(true);
    }

    #[tokio::test]
    async fn queued_rechecks_count_once_and_observe_fills_before_or_after_waiting() {
        use std::future::Future;
        use std::task::Context;

        let cache = ExactCache::new(CacheConfig::default());
        let mut owner = pending(&cache, 1);
        assert!(owner.wait_for_turn().await.is_none());
        let mut request = pending(&cache, 1);
        assert!(request.lookup().is_none());
        assert!(request.lookup().is_none());
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        {
            let wait = request.wait_for_turn();
            tokio::pin!(wait);
            assert!(wait.as_mut().poll(&mut context).is_pending());
            for key in 2..4 {
                store(&cache, key, false);
                assert!(wait.as_mut().poll(&mut context).is_pending());
                let snapshot = ExactCache::snapshot(Some(&cache));
                assert_eq!(snapshot.misses, 2);
                assert_eq!(snapshot.hits, 0);
            }
            store(&cache, 1, false);
            assert!(wait.as_mut().poll(&mut context).is_ready());
        }
        assert!(request.lookup().is_some());
        let snapshot = ExactCache::snapshot(Some(&cache));
        assert_eq!(snapshot.misses, 1);
        assert_eq!(snapshot.hits, 1);
        assert_eq!(snapshot.coalesced, 1);
        assert_eq!(snapshot.waiters, 0);
        drop(owner);

        let mut late = pending(&cache, 4);
        assert!(late.lookup().is_none());
        store(&cache, 4, false);
        tokio::time::timeout(Duration::from_secs(1), late.wait_for_turn())
            .await
            .expect("fill before wait registration must be observed");
        let snapshot = ExactCache::snapshot(Some(&cache));
        assert_eq!(snapshot.misses, 1);
        assert_eq!(snapshot.hits, 2);
    }

    #[tokio::test]
    async fn ownership_releases_on_failure_cancellation_and_uncacheable_capture() {
        use std::future::Future;
        use std::task::Context;

        let cache = ExactCache::new(CacheConfig::default());
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        let mut owner = pending(&cache, 1);
        assert!(owner.wait_for_turn().await.is_none());
        let mut cancelled = pending(&cache, 1);
        {
            let wait = cancelled.wait_for_turn();
            tokio::pin!(wait);
            assert!(wait.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(ExactCache::snapshot(Some(&cache)).waiters, 1);
        drop(cancelled);
        assert_eq!(ExactCache::snapshot(Some(&cache)).waiters, 0);
        assert_eq!(ExactCache::snapshot(Some(&cache)).inflight_keys, 1);

        let mut follower = pending(&cache, 1);
        {
            let wait = follower.wait_for_turn();
            tokio::pin!(wait);
            assert!(wait.as_mut().poll(&mut context).is_pending());
            let headers = headers();
            assert!(
                owner
                    .capture(StatusCode::BAD_GATEWAY, &headers, &headers)
                    .is_none()
            );
            assert!(matches!(
                wait.as_mut().poll(&mut context),
                std::task::Poll::Ready(None)
            ));
        }
        assert_eq!(ExactCache::snapshot(Some(&cache)).inflight_keys, 1);
        assert_eq!(ExactCache::snapshot(Some(&cache)).waiters, 0);
        let mut no_store = headers();
        no_store.insert("cache-control", "no-store".parse().unwrap());
        assert!(
            follower
                .capture(StatusCode::OK, &no_store, &no_store)
                .is_none()
        );
        assert_eq!(ExactCache::snapshot(Some(&cache)).inflight_keys, 0);
        assert_eq!(ExactCache::snapshot(Some(&cache)).stores, 0);
        assert_eq!(ExactCache::snapshot(Some(&cache)).coalesced, 0);

        let mut owner = pending(&cache, 1);
        assert!(owner.wait_for_turn().await.is_none());
        let headers = headers();
        let mut capture = owner.capture(StatusCode::OK, &headers, &headers).unwrap();
        assert!(!capture.observe(&vec![0; ENTRY_BYTES]));
        assert!(!capture.observe(&vec![0; ENTRY_BYTES]));
        drop(capture);
        let counts = ExactCache::snapshot(Some(&cache)).not_stored;
        assert_eq!(counts.status, 1);
        assert_eq!(counts.response_cache_control, 1);
        assert_eq!(counts.size, 1);
        assert_eq!(counts.incomplete_or_unsafe, 0);
        assert_eq!(ExactCache::snapshot(Some(&cache)).inflight_keys, 0);
        assert_eq!(ExactCache::snapshot(Some(&cache)).retained_bytes, 0);
    }

    #[tokio::test]
    async fn bounded_flight_table_keeps_existing_owners_and_releases_capacity() {
        let cache = ExactCache::new(CacheConfig::default());
        let mut owners = Vec::new();
        for key in 0..MAX_ENTRIES as u16 {
            let mut owner = pending(&cache, key);
            assert!(owner.wait_for_turn().await.is_none());
            owners.push(owner);
        }
        let mut bypass = pending(&cache, MAX_ENTRIES as u16);
        assert!(bypass.wait_for_turn().await.is_none());
        assert!(bypass.wait_for_turn().await.is_none());
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).inflight_keys,
            MAX_ENTRIES
        );
        assert_eq!(ExactCache::snapshot(Some(&cache)).coordination_bypasses, 1);
        drop(bypass);
        drop(owners.pop());
        let mut replacement = pending(&cache, MAX_ENTRIES as u16);
        assert!(replacement.wait_for_turn().await.is_none());
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).inflight_keys,
            MAX_ENTRIES
        );
        assert_eq!(ExactCache::snapshot(Some(&cache)).coordination_bypasses, 1);
        drop(replacement);
        drop(owners);
        assert_eq!(ExactCache::snapshot(Some(&cache)).inflight_keys, 0);
    }

    #[tokio::test]
    async fn pinned_replays_and_polled_frames_keep_budget_after_eviction() {
        let cache = ExactCache::new(CacheConfig::default());
        let mut replays = Vec::new();
        for key in 0..16 {
            store(&cache, key, true);
            replays.push(pending(&cache, key).lookup().unwrap());
        }
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).retained_bytes,
            BUDGET_BYTES
        );
        let mut response = replays.pop().unwrap();
        let bytes = response
            .body_mut()
            .frame()
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap();
        drop(response);
        let headers = headers();
        assert!(
            pending(&cache, 99)
                .capture(StatusCode::OK, &headers, &headers)
                .is_none()
        );
        assert_eq!(ExactCache::snapshot(Some(&cache)).entries, 0);
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).retained_bytes,
            BUDGET_BYTES
        );
        drop(bytes);
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).retained_bytes,
            BUDGET_BYTES - ENTRY_BYTES
        );
        let capture = pending(&cache, 99)
            .capture(StatusCode::OK, &headers, &headers)
            .unwrap();
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).retained_bytes,
            BUDGET_BYTES
        );
        drop(capture);
        drop(replays);
        assert_eq!(ExactCache::snapshot(Some(&cache)).retained_bytes, 0);
    }

    #[test]
    fn captures_reserve_nonblocking_and_entries_trim_unused_bytes_with_count_ceiling() {
        let cache = ExactCache::new(CacheConfig::default());
        let headers = headers();
        let captures: Vec<_> = (0..16)
            .map(|key| {
                pending(&cache, key)
                    .capture(StatusCode::OK, &headers, &headers)
                    .unwrap()
            })
            .collect();
        assert!(
            pending(&cache, 17)
                .capture(StatusCode::OK, &headers, &headers)
                .is_none()
        );
        let snapshot = ExactCache::snapshot(Some(&cache));
        assert_eq!(snapshot.budget_bypasses, 1);
        assert!(
            serde_json::to_value(snapshot.not_stored)
                .unwrap()
                .as_object()
                .unwrap()
                .values()
                .all(|value| value == 0)
        );
        assert_eq!(
            ExactCache::snapshot(Some(&cache)).retained_bytes,
            BUDGET_BYTES
        );
        drop(captures);
        assert_eq!(
            ExactCache::snapshot(Some(&cache))
                .not_stored
                .incomplete_or_unsafe,
            16
        );
        for key in 0..200 {
            store(&cache, key, false);
        }
        let snapshot = ExactCache::snapshot(Some(&cache));
        assert_eq!(snapshot.entries, MAX_ENTRIES);
        assert_eq!(
            snapshot.retained_bytes,
            MAX_ENTRIES * (PREFIX.len() + SUFFIX.len() + 1 + stored_size(&headers))
        );
        assert_eq!(snapshot.evictions, 72);
        assert!(pending(&cache, 0).lookup().is_none());
        assert!(pending(&cache, 199).lookup().is_some());
    }

    #[test]
    fn key_frames_all_duplicate_header_values_and_exact_body_bytes() {
        let cache = ExactCache::new(CacheConfig::default());
        let body = br#"{"model":"x","messages":[{"role":"user","content":"x"}]}"#;
        let url = "http://localhost/v1/chat/completions".parse().unwrap();
        let mut headers = headers();
        headers.append("x-test", "a".parse().unwrap());
        headers.append("x-test", "bc".parse().unwrap());
        let key = cache
            .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
            .unwrap()
            .key;
        for value in ["0", "1", "2"] {
            headers.append("x-stainless-retry-count", value.parse().unwrap());
            assert_eq!(
                key,
                cache
                    .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
                    .unwrap()
                    .key
            );
        }
        headers.insert("x-stainless-read-timeout", "10".parse().unwrap());
        assert_ne!(
            key,
            cache
                .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
                .unwrap()
                .key
        );
        headers.remove("x-stainless-read-timeout");
        headers.remove("x-test");
        headers.append("x-test", "ab".parse().unwrap());
        headers.append("x-test", "c".parse().unwrap());
        assert_ne!(
            key,
            cache
                .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
                .unwrap()
                .key
        );
        let a = cache
            .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
            .unwrap()
            .key;
        headers.remove("x-test");
        headers.append("x-test", "c".parse().unwrap());
        headers.append("x-test", "ab".parse().unwrap());
        assert_ne!(
            a,
            cache
                .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
                .unwrap()
                .key
        );
        let a = cache
            .request(0, &url, &headers, Endpoint::ChatCompletions, body, false)
            .unwrap()
            .key;
        assert_ne!(
            a,
            cache
                .request(
                    0,
                    &url,
                    &headers,
                    Endpoint::ChatCompletions,
                    &[body.as_slice(), b" "].concat(),
                    false,
                )
                .unwrap()
                .key
        );
    }

    #[test]
    fn bypass_reasons_are_fixed_order_and_preserve_short_text_eligibility() {
        use serde_json::json;
        let chat = json!({"model":"x", "messages":[{"role":"user", "content":"x"}]});
        let history = vec![json!({"role":"user", "content":"x"}); 4];
        let mut tools = chat.clone();
        tools["tools"] = json!([]);
        tools["messages"] = json!(history);
        let mut long = chat.clone();
        long["messages"] = tools["messages"].clone();
        long["unknown"] = json!(true);
        for (endpoint, body, forbidden, expected) in [
            (Endpoint::ChatCompletions, chat.clone(), false, None),
            (Endpoint::Models, json!({}), false, Some(Bypass::Endpoint)),
            (
                Endpoint::Models,
                json!("x".repeat(REQUEST_BYTES)),
                false,
                Some(Bypass::Size),
            ),
            (
                Endpoint::Models,
                json!("x".repeat(REQUEST_BYTES)),
                true,
                Some(Bypass::RequestCacheControl),
            ),
            (
                Endpoint::ChatCompletions,
                tools,
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::ChatCompletions,
                long,
                false,
                Some(Bypass::History),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"tool","content":"x"}]}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"assistant","content":"x","tool_calls":[]}]}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::Messages,
                json!({"messages":[{"role":"user","content":[{"type":"tool_result"}]}]}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::Responses,
                json!({"input":"x","store":false}),
                false,
                None,
            ),
            (
                Endpoint::Responses,
                json!({"input":"x"}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::Responses,
                json!({"input":"x","store":false,"previous_response_id":"x"}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::Responses,
                json!({"input":[{"type":"function_call_output"}],"store":false}),
                false,
                Some(Bypass::ToolsState),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"user","content":[{"type":"image_url"}]}]}),
                false,
                Some(Bypass::UnsupportedShape),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"user","content":"x"}],"n":2}),
                false,
                Some(Bypass::UnsupportedShape),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"user","content":"x"}],"stream_options":{"include_usage":"yes"}}),
                false,
                Some(Bypass::UnsupportedShape),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[{"role":"user","content":"x"}],"unknown":true}),
                false,
                Some(Bypass::UnsupportedShape),
            ),
            (
                Endpoint::ChatCompletions,
                json!({"messages":[]}),
                false,
                Some(Bypass::UnsupportedShape),
            ),
        ] {
            assert_eq!(
                bypass_reason(endpoint, &serde_json::to_vec(&body).unwrap(), 3, forbidden),
                expected,
                "{endpoint:?}: {body}"
            );
        }
        assert_eq!(
            bypass_reason(Endpoint::ChatCompletions, b"{", 3, false),
            Some(Bypass::UnsupportedShape)
        );
    }
}
