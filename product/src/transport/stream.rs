use std::io;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, Response, StatusCode};
use futures_util::StreamExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;

use crate::admission::Hold;
use crate::config::{CancelPolicy, Endpoint};
use crate::metrics::{Metrics, TerminalReason};
use crate::protocol::{EndpointObserver, ObservedUsage};
use crate::transport::body_budget::BudgetedBody;
use crate::transport::{headers, upstream::UpstreamClient};

const DELIVERY_ITEMS: usize = 8;
const DELIVERY_BYTES: usize = 64 * 1024;
const DELIVERY_CHUNK_BYTES: usize = 16 * 1024;
const MAX_EVENT_METADATA: usize = 256 * 1024;
const EVENT_NAME_BYTES: usize = 32;
const BOM_BYTES: usize = 3;
const EVENT_STORAGE_BYTES: usize = MAX_EVENT_METADATA - EVENT_NAME_BYTES - BOM_BYTES;

pub enum HeadError {
    Upstream,
    Deadline,
    Admission(crate::admission::AcquireError),
}

pub type HeadSender = oneshot::Sender<Result<Response<Body>, HeadError>>;

pub struct ForwardRequest {
    pub cache: Option<crate::cache::Pending>,
    pub client: UpstreamClient,
    pub method: Method,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: BudgetedBody,
    pub endpoint: Endpoint,
    pub cancel_policy: CancelPolicy,
    pub retry_transient_429: bool,
    pub deadline: Instant,
    pub admission_hold: Hold,
    pub prestart_gate: Option<tokio::sync::watch::Receiver<bool>>,
    pub stop: tokio::sync::watch::Receiver<bool>,
    pub metrics: Metrics,
    pub head: HeadSender,
    pub downstream_disconnect: tokio::sync::watch::Receiver<bool>,
}

enum BodyTerminal {
    Eof,
    Failure(&'static str),
}

struct DeliveryReceiver {
    payload: mpsc::Receiver<Bytes>,
    terminal: Option<oneshot::Receiver<BodyTerminal>>,
}

fn downstream_body(
    payload: mpsc::Receiver<Bytes>,
    terminal: oneshot::Receiver<BodyTerminal>,
) -> Body {
    let stream = futures_util::stream::unfold(
        DeliveryReceiver {
            payload,
            terminal: Some(terminal),
        },
        |mut delivery| async move {
            if let Some(bytes) = delivery.payload.recv().await {
                return Some((Ok::<_, io::Error>(bytes), delivery));
            }
            let terminal = delivery.terminal.take()?;
            match terminal.await {
                Ok(BodyTerminal::Eof) => None,
                Ok(BodyTerminal::Failure(reason)) => {
                    Some((Err(io::Error::other(reason)), delivery))
                }
                Err(_) => Some((
                    Err(io::Error::other("upstream worker ended before body EOF")),
                    delivery,
                )),
            }
        },
    );
    Body::from_stream(stream)
}

fn signal_body(sender: &mut Option<oneshot::Sender<BodyTerminal>>, terminal: BodyTerminal) {
    if let Some(sender) = sender.take() {
        let _ = sender.send(terminal);
    }
}

pub async fn forward(request: ForwardRequest) {
    let ForwardRequest {
        cache,
        client,
        method,
        url,
        headers: request_headers,
        body,
        endpoint,
        cancel_policy,
        retry_transient_429,
        deadline,
        admission_hold,
        mut prestart_gate,
        mut stop,
        metrics,
        mut head,
        mut downstream_disconnect,
    } = request;
    let mut guard = WorkerGuard::new(metrics.clone(), admission_hold);
    let mut disconnected = false;
    let mut retried = false;
    let (response, prefix) = loop {
        let send = client.send(
            method.clone(),
            url.clone(),
            request_headers.clone(),
            body.clone(),
        );
        tokio::pin!(send);
        let mut attempt_started = false;
        let mut response = loop {
            tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => {
                    let _ = head.send(Err(HeadError::Deadline));
                    guard.finish(TerminalReason::Deadline, None);
                    return;
                }
                () = wait_for_signal(&mut downstream_disconnect), if !disconnected => {
                    disconnected = true;
                    if !attempt_started {
                        guard.finish(TerminalReason::DownstreamClose, None);
                        return;
                    }
                    match cancel_policy {
                        CancelPolicy::Close => {
                            guard.finish(TerminalReason::DownstreamClose, None);
                            return;
                        }
                        CancelPolicy::Drain => guard.start_draining(),
                    }
                }
                () = head.closed(), if !disconnected => {
                    disconnected = true;
                    if !attempt_started {
                        guard.finish(TerminalReason::DownstreamClose, None);
                        return;
                    }
                    match cancel_policy {
                        CancelPolicy::Close => {
                            guard.finish(TerminalReason::DownstreamClose, None);
                            return;
                        }
                        CancelPolicy::Drain => guard.start_draining(),
                    }
                }
                () = wait_for_signal(&mut stop), if !attempt_started => {
                    guard.finish(TerminalReason::Shutdown, None);
                    return;
                }
                response = async {
                    if !attempt_started {
                        if let Some(gate) = prestart_gate.as_mut() { wait_for_signal(gate).await; }
                        guard.admission_hold.as_mut().expect("worker owns admission").start();
                        attempt_started = true;
                        metrics.record_upstream_attempt();
                    }
                    send.as_mut().await
                } => match response {
                    Ok(response) => break response,
                    Err(_) => {
                        let _ = head.send(Err(HeadError::Upstream));
                        guard.finish(TerminalReason::UpstreamError, None);
                        return;
                    }
                }
            }
        };

        let mut prefix = RetryPrefix::default();
        if response.status() == axum::http::StatusCode::TOO_MANY_REQUESTS {
            let delay = crate::admission::retry::header_delay(
                response.headers(),
                std::time::SystemTime::now(),
            );
            if let Some(delay) = delay {
                guard
                    .admission_hold
                    .as_ref()
                    .expect("worker owns admission")
                    .cooldown(delay);
            }
            let needs_body = crate::admission::retry::missing_timing(response.headers())
                || (retry_transient_429
                    && !retried
                    && !disconnected
                    && !crate::admission::retry::server_forbids_retry(response.headers())
                    && crate::admission::retry::timing_allows_retry(response.headers()));
            let probing = async {
                if needs_body {
                    probe_rejection(&mut response).await
                } else {
                    Ok(RetryPrefix::default())
                }
            };
            tokio::pin!(probing);
            prefix = loop {
                tokio::select! {
                    biased;
                    () = tokio::time::sleep_until(deadline) => {
                        let _ = head.send(Err(HeadError::Deadline));
                        guard.finish(TerminalReason::Deadline, None); return;
                    }
                    () = head.closed(), if !disconnected => {
                        disconnected = true;
                        if cancel_policy == CancelPolicy::Close { guard.finish(TerminalReason::DownstreamClose, None); return; }
                        guard.start_draining();
                    }
                    () = wait_for_signal(&mut downstream_disconnect), if !disconnected => {
                        disconnected = true;
                        if cancel_policy == CancelPolicy::Close { guard.finish(TerminalReason::DownstreamClose, None); return; }
                        guard.start_draining();
                    }
                    prefix = &mut probing => match prefix {
                        Ok(prefix) => break prefix,
                        Err(_) => {
                            let _ = head.send(Err(HeadError::Upstream));
                            guard.finish(TerminalReason::UpstreamError, None);
                            return;
                        }
                    },
                }
            };
            // The probe future's response borrow ends before inspecting headers or dropping the body.
        }
        let recognized = prefix.complete
            && crate::admission::retry::transient(response.headers(), &prefix.bytes);
        if recognized && crate::admission::retry::missing_timing(response.headers()) {
            guard
                .admission_hold
                .as_ref()
                .expect("worker owns admission")
                .cooldown(crate::admission::retry::fallback_delay());
        }
        if recognized
            && retry_transient_429
            && !retried
            && !disconnected
            && !crate::admission::retry::server_forbids_retry(response.headers())
            && crate::admission::retry::timing_allows_retry(response.headers())
        {
            // Complete rejected response EOF was observed; drop the local HTTP body before releasing its hold.
            metrics.record_response_body_eof();
            drop(prefix);
            drop(response);
            let previous = guard.admission_hold.take().expect("worker owns admission");
            let acquire = previous.retry();
            tokio::pin!(acquire);
            let next = tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => {
                    let _ = head.send(Err(HeadError::Deadline));
                    guard.finish(TerminalReason::Deadline, None); return;
                }
                () = wait_for_signal(&mut stop) => { guard.finish(TerminalReason::Shutdown, None); return; }
                () = head.closed() => { guard.finish(TerminalReason::DownstreamClose, None); return; }
                () = wait_for_signal(&mut downstream_disconnect) => { guard.finish(TerminalReason::DownstreamClose, None); return; }
                next = &mut acquire => next,
            };
            match next {
                Ok(hold) => guard.admission_hold = Some(hold),
                Err(error) => {
                    let reason = if error == crate::admission::AcquireError::Deadline {
                        TerminalReason::Deadline
                    } else {
                        TerminalReason::AdmissionRejected
                    };
                    let _ = head.send(Err(HeadError::Admission(error)));
                    guard.finish(reason, None);
                    return;
                }
            }
            retried = true;
            continue;
        }
        break (response, prefix);
    };
    drop(body);
    let status = response.status();
    let mut response_headers = response.headers().clone();
    let representation = observable_representation(status, &response_headers);
    headers::prepare_response(&mut response_headers);
    let mut capture = cache
        .filter(|_| !disconnected)
        .and_then(|cache| cache.capture(status, response.headers(), &response_headers));
    let (body_tx, body_rx) = mpsc::channel::<Bytes>(DELIVERY_ITEMS);
    let (body_terminal_tx, body_terminal_rx) = oneshot::channel();
    let mut body_terminal_tx = Some(body_terminal_tx);
    let mut downstream = Response::new(downstream_body(body_rx, body_terminal_rx));
    *downstream.status_mut() = status;
    *downstream.headers_mut() = response_headers;
    let _ = head.send(Ok(downstream));

    let delivery_budget = Arc::new(Semaphore::new(DELIVERY_BYTES));
    let prefix_items = (!prefix.bytes.is_empty())
        .then(|| Ok(Bytes::from(prefix.bytes)))
        .into_iter()
        .chain(prefix.extra.map(Ok));
    let mut upstream = futures_util::stream::iter(prefix_items).chain(response.bytes_stream());
    let mut observer = ResponseObserver::new(endpoint, representation, capture.is_some());
    let mut downstream_open = !disconnected;
    let mut first_body_recorded = false;
    let mut output_delta_recorded = false;
    let mut terminal_marker_recorded = false;
    let mut overflow_recorded = false;

    loop {
        let next = if downstream_open {
            tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => {
                    signal_body(&mut body_terminal_tx, BodyTerminal::Failure("gateway request deadline"));
                    guard.finish(TerminalReason::Deadline, None);
                    return;
                }
                () = wait_for_signal(&mut downstream_disconnect), if !disconnected => {
                    disconnected = true;
                    match cancel_policy {
                        CancelPolicy::Close => {
                            signal_body(&mut body_terminal_tx, BodyTerminal::Failure("downstream connection closed"));
                            guard.finish(TerminalReason::DownstreamClose, None);
                            return;
                        }
                        CancelPolicy::Drain => {
                            capture = None;
                            downstream_open = false;
                            guard.start_draining();
                            continue;
                        }
                    }
                }
                () = body_tx.closed() => {
                    match cancel_policy {
                        CancelPolicy::Close => {
                            signal_body(&mut body_terminal_tx, BodyTerminal::Failure("downstream response body closed"));
                            guard.finish(TerminalReason::DownstreamClose, None);
                            return;
                        }
                        CancelPolicy::Drain => {
                            capture = None;
                            downstream_open = false;
                            guard.start_draining();
                            continue;
                        }
                    }
                }
                next = upstream.next() => next,
            }
        } else {
            tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => {
                    signal_body(&mut body_terminal_tx, BodyTerminal::Failure("gateway request deadline"));
                    guard.finish(TerminalReason::Deadline, None);
                    return;
                }
                next = upstream.next() => next,
            }
        };

        let Some(next) = next else {
            metrics.record_response_body_eof();
            let (usage, cache_complete) = observer.finish();
            if downstream_open && !body_tx.is_closed() && !*downstream_disconnect.borrow() {
                if let Some(capture) = capture.take() {
                    capture.commit(cache_complete);
                }
            }
            signal_body(&mut body_terminal_tx, BodyTerminal::Eof);
            guard.finish(TerminalReason::BodyEof, usage);
            return;
        };
        let bytes = match next {
            Ok(bytes) => bytes,
            Err(_) => {
                signal_body(
                    &mut body_terminal_tx,
                    BodyTerminal::Failure("upstream response stream failed"),
                );
                guard.finish(TerminalReason::UpstreamError, None);
                return;
            }
        };
        if !bytes.is_empty() && !first_body_recorded {
            first_body_recorded = true;
            metrics.record_first_body_byte();
        }
        if capture
            .as_mut()
            .is_some_and(|capture| !capture.observe(&bytes))
        {
            capture = None;
        }
        observer.observe(&bytes);
        if observer.first_output_delta() && !output_delta_recorded {
            output_delta_recorded = true;
            metrics.record_first_observed_output_delta();
        }
        if observer.terminal_marker() && !terminal_marker_recorded {
            terminal_marker_recorded = true;
            metrics.record_terminal_marker();
        }
        if observer.overflowed() && !overflow_recorded {
            overflow_recorded = true;
            metrics.record_observer_overflow();
        }

        if downstream_open {
            match deliver(&body_tx, &delivery_budget, &metrics, &bytes, deadline).await {
                Delivery::Delivered => {}
                Delivery::Closed => match cancel_policy {
                    CancelPolicy::Close => {
                        signal_body(
                            &mut body_terminal_tx,
                            BodyTerminal::Failure("downstream response body closed"),
                        );
                        guard.finish(TerminalReason::DownstreamClose, None);
                        return;
                    }
                    CancelPolicy::Drain => {
                        capture = None;
                        downstream_open = false;
                        guard.start_draining();
                    }
                },
                Delivery::Deadline => {
                    signal_body(
                        &mut body_terminal_tx,
                        BodyTerminal::Failure("gateway request deadline"),
                    );
                    guard.finish(TerminalReason::Deadline, None);
                    return;
                }
            }
        }
    }
}

#[derive(Default)]
struct RetryPrefix {
    bytes: Vec<u8>,
    extra: Option<Bytes>,
    complete: bool,
}
async fn probe_rejection(response: &mut reqwest::Response) -> Result<RetryPrefix, reqwest::Error> {
    use crate::admission::retry::MAX_ERROR_BODY;
    let mut prefix = RetryPrefix::default();
    // Unclassifiable, encoded or declared oversized responses remain purely streaming.
    if !crate::admission::retry::classifiable_representation(response.headers())
        || response
            .content_length()
            .is_some_and(|len| len > MAX_ERROR_BODY as u64)
        || response
            .headers()
            .get_all("content-encoding")
            .iter()
            // Keep the probe's stricter spelling rule; the shared classifier is case-insensitive.
            .any(|h| h != "identity")
    {
        return Ok(prefix);
    }
    loop {
        match response.chunk().await {
            Ok(Some(bytes)) if prefix.bytes.len().saturating_add(bytes.len()) <= MAX_ERROR_BODY => {
                prefix.bytes.extend_from_slice(&bytes);
            }
            Ok(Some(bytes)) => {
                prefix.extra = Some(bytes);
                return Ok(prefix);
            }
            Ok(None) => {
                prefix.complete = true;
                return Ok(prefix);
            }
            Err(error) => return Err(error),
        }
    }
}

enum Delivery {
    Delivered,
    Closed,
    Deadline,
}

async fn deliver(
    sender: &mpsc::Sender<Bytes>,
    budget: &Arc<Semaphore>,
    metrics: &Metrics,
    source: &[u8],
    deadline: Instant,
) -> Delivery {
    for slice in source.chunks(DELIVERY_CHUNK_BYTES) {
        let permits = u32::try_from(slice.len()).expect("delivery chunk bound fits u32");
        let permit = tokio::select! {
            biased;
            () = tokio::time::sleep_until(deadline) => return Delivery::Deadline,
            () = sender.closed() => return Delivery::Closed,
            permit = budget.clone().acquire_many_owned(permits) => match permit {
                Ok(permit) => permit,
                Err(_) => return Delivery::Closed,
            },
        };
        let bytes = queued_bytes(slice, permit, metrics.clone());
        let sent = tokio::select! {
            biased;
            () = tokio::time::sleep_until(deadline) => return Delivery::Deadline,
            sent = sender.send(bytes) => sent,
        };
        if sent.is_err() {
            return Delivery::Closed;
        }
    }
    Delivery::Delivered
}

struct QueuedBytes {
    bytes: Vec<u8>,
    permit: Option<OwnedSemaphorePermit>,
    metrics: Metrics,
}

impl AsRef<[u8]> for QueuedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for QueuedBytes {
    fn drop(&mut self) {
        self.metrics.queued_bytes_removed(self.bytes.len());
        self.permit.take();
    }
}

fn queued_bytes(source: &[u8], permit: OwnedSemaphorePermit, metrics: Metrics) -> Bytes {
    let bytes = source.to_vec();
    metrics.queued_bytes_added(bytes.len());
    Bytes::from_owner(QueuedBytes {
        bytes,
        permit: Some(permit),
        metrics,
    })
}

struct WorkerGuard {
    metrics: Metrics,
    admission_hold: Option<Hold>,
    draining: bool,
    terminal_reason: TerminalReason,
    usage: Option<ObservedUsage>,
}

impl WorkerGuard {
    fn new(metrics: Metrics, permit: Hold) -> Self {
        metrics.worker_started();
        Self {
            metrics,
            admission_hold: Some(permit),
            draining: false,
            terminal_reason: TerminalReason::Shutdown,
            usage: None,
        }
    }

    fn start_draining(&mut self) {
        if !self.draining {
            self.draining = true;
            self.metrics.draining_started();
        }
    }

    fn finish(&mut self, reason: TerminalReason, usage: Option<ObservedUsage>) {
        self.terminal_reason = reason;
        self.usage = usage;
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        if self.draining {
            self.metrics.draining_finished();
        }
        if let Some(hold) = self.admission_hold.as_mut() {
            hold.usage(self.usage);
        }
        self.admission_hold.take();
        self.metrics
            .worker_finished(self.terminal_reason, self.usage);
    }
}

fn observable_representation(status: StatusCode, headers: &HeaderMap) -> Option<&'static str> {
    if status != StatusCode::OK {
        return None;
    }
    let mut encodings = headers.get_all("content-encoding").iter();
    if let Some(encoding) = encodings.next() {
        if !encoding.to_str().ok()?.eq_ignore_ascii_case("identity") || encodings.next().is_some() {
            return None;
        }
    }
    let mut types = headers.get_all("content-type").iter();
    let media = types.next()?.to_str().ok()?.split(';').next()?.trim();
    if types.next().is_some() {
        return None;
    }
    if media.eq_ignore_ascii_case("application/json") {
        Some("application/json")
    } else if media.eq_ignore_ascii_case("text/event-stream") {
        Some("text/event-stream")
    } else {
        None
    }
}

enum ResponseObserver {
    Sse(SseDecoder),
    Json {
        endpoint: Endpoint,
        storage: Box<[u8]>,
        used: usize,
        overflowed: bool,
        cache: bool,
    },
    Unsupported,
}

impl ResponseObserver {
    fn new(endpoint: Endpoint, representation: Option<&str>, cache: bool) -> Self {
        if matches!(endpoint, Endpoint::Models | Endpoint::CountTokens) {
            return Self::Unsupported;
        }
        match representation {
            Some(media) if media.eq_ignore_ascii_case("text/event-stream") => {
                Self::Sse(SseDecoder::new(endpoint, cache))
            }
            Some(media) if media.eq_ignore_ascii_case("application/json") => Self::Json {
                endpoint,
                storage: vec![0; MAX_EVENT_METADATA].into_boxed_slice(),
                used: 0,
                overflowed: false,
                cache,
            },
            _ => Self::Unsupported,
        }
    }

    fn observe(&mut self, bytes: &[u8]) {
        match self {
            Self::Sse(sse) => sse.observe(bytes),
            Self::Json {
                storage,
                used,
                overflowed,
                ..
            } if !*overflowed => {
                if bytes.len() > storage.len() - *used {
                    *overflowed = true;
                    return;
                }
                storage[*used..*used + bytes.len()].copy_from_slice(bytes);
                *used += bytes.len();
            }
            _ => {}
        }
    }

    fn first_output_delta(&self) -> bool {
        matches!(self, Self::Sse(sse) if sse.endpoint.first_output_delta())
    }

    fn terminal_marker(&self) -> bool {
        matches!(self, Self::Sse(sse) if sse.endpoint.terminal_marker())
    }

    fn overflowed(&self) -> bool {
        match self {
            Self::Sse(sse) => sse.overflowed,
            Self::Json { overflowed, .. } => *overflowed,
            Self::Unsupported => false,
        }
    }

    // Called only at clean HTTP EOF. JSON parsing never delays the first body chunk.
    fn finish(&self) -> (Option<ObservedUsage>, bool) {
        match self {
            Self::Sse(sse) if !sse.overflowed => (
                sse.endpoint.finish(),
                sse.at_event_boundary() && sse.endpoint.cache_complete(),
            ),
            Self::Json {
                endpoint,
                storage,
                used,
                overflowed: false,
                cache,
            } => {
                let Ok(value) = serde_json::from_slice::<serde_json::Value>(&storage[..*used])
                else {
                    return (None, false);
                };
                (
                    crate::protocol::json_usage(*endpoint, &value),
                    *cache && crate::cache::complete_json(*endpoint, &value),
                )
            }
            _ => (None, false),
        }
    }
}

struct SseDecoder {
    endpoint: EndpointObserver,
    storage: Box<[u8]>,
    used: usize,
    line_start: usize,
    line_kind: LineKind,
    event: SseEvent,
    event_name: [u8; EVENT_NAME_BYTES],
    event_name_len: usize,
    event_name_overflow: bool,
    has_data: bool,
    pending_cr: bool,
    bom_checked: bool,
    bom_prefix: [u8; BOM_BYTES],
    bom_prefix_len: usize,
    overflowed: bool,
}

#[derive(Clone, Copy)]
enum LineKind {
    Field,
    Data { strip_space: bool },
    Event { strip_space: bool },
    Ignore,
}

#[derive(Clone, Copy)]
enum SseEvent {
    Default,
    MessageStart,
    MessageDelta,
    ContentBlockDelta,
    MessageStop,
    Error,
    Unknown,
}

impl SseEvent {
    fn from_bytes(bytes: &[u8], overflowed: bool) -> Self {
        if overflowed {
            return Self::Unknown;
        }
        match bytes {
            b"" => Self::Default,
            b"message_start" => Self::MessageStart,
            b"message_delta" => Self::MessageDelta,
            b"content_block_delta" => Self::ContentBlockDelta,
            b"message_stop" => Self::MessageStop,
            b"error" => Self::Error,
            _ => Self::Unknown,
        }
    }

    fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::MessageStart => Some("message_start"),
            Self::MessageDelta => Some("message_delta"),
            Self::ContentBlockDelta => Some("content_block_delta"),
            Self::MessageStop => Some("message_stop"),
            Self::Error => Some("error"),
            Self::Unknown => Some("unknown"),
        }
    }
}

impl SseDecoder {
    fn new(endpoint: Endpoint, cache: bool) -> Self {
        Self {
            endpoint: EndpointObserver::new(endpoint, cache),
            storage: vec![0; EVENT_STORAGE_BYTES].into_boxed_slice(),
            used: 0,
            line_start: 0,
            line_kind: LineKind::Field,
            event: SseEvent::Default,
            event_name: [0; EVENT_NAME_BYTES],
            event_name_len: 0,
            event_name_overflow: false,
            has_data: false,
            pending_cr: false,
            bom_checked: false,
            bom_prefix: [0; BOM_BYTES],
            bom_prefix_len: 0,
            overflowed: false,
        }
    }

    fn at_event_boundary(&self) -> bool {
        self.used == 0
            && !self.has_data
            && matches!(self.line_kind, LineKind::Field)
            && matches!(self.event, SseEvent::Default)
            && self.event_name_len == 0
            && self.bom_checked
            && self.bom_prefix_len == 0
    }

    fn observe(&mut self, mut bytes: &[u8]) {
        if self.overflowed {
            return;
        }
        if !self.bom_checked {
            let mut consumed = 0;
            while consumed < bytes.len() && self.bom_prefix_len < BOM_BYTES {
                self.bom_prefix[self.bom_prefix_len] = bytes[consumed];
                self.bom_prefix_len += 1;
                consumed += 1;
                let bom = b"\xef\xbb\xbf";
                if !bom.starts_with(&self.bom_prefix[..self.bom_prefix_len]) {
                    self.bom_checked = true;
                    let prefix = self.bom_prefix;
                    let prefix_len = self.bom_prefix_len;
                    self.bom_prefix_len = 0;
                    self.process(&prefix[..prefix_len]);
                    break;
                }
                if self.bom_prefix_len == BOM_BYTES {
                    self.bom_checked = true;
                    self.bom_prefix_len = 0;
                    break;
                }
            }
            bytes = &bytes[consumed..];
        }
        self.process(bytes);
    }

    fn process(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.overflowed {
                return;
            }
            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.finish_line();
                    self.pending_cr = true;
                }
                b'\n' => self.finish_line(),
                _ => self.process_line_byte(byte),
            }
        }
    }

    fn process_line_byte(&mut self, byte: u8) {
        match self.line_kind {
            LineKind::Field if self.used == self.line_start && byte == b':' => {
                self.line_kind = LineKind::Ignore;
            }
            LineKind::Field if byte == b':' => {
                let is_data = &self.storage[self.line_start..self.used] == b"data";
                let is_event = &self.storage[self.line_start..self.used] == b"event";
                self.used = self.line_start;
                if is_data {
                    if self.has_data {
                        self.push_storage(b'\n');
                    }
                    self.line_kind = LineKind::Data { strip_space: true };
                } else if is_event {
                    self.reset_event_name();
                    self.line_kind = LineKind::Event { strip_space: true };
                } else {
                    self.line_kind = LineKind::Ignore;
                }
            }
            LineKind::Field => self.push_storage(byte),
            LineKind::Data { strip_space: true } if byte == b' ' => {
                self.line_kind = LineKind::Data { strip_space: false };
            }
            LineKind::Data { .. } => {
                self.line_kind = LineKind::Data { strip_space: false };
                self.push_storage(byte);
            }
            LineKind::Event { strip_space: true } if byte == b' ' => {
                self.line_kind = LineKind::Event { strip_space: false };
            }
            LineKind::Event { .. } => {
                self.line_kind = LineKind::Event { strip_space: false };
                if self.event_name_len < EVENT_NAME_BYTES {
                    self.event_name[self.event_name_len] = byte;
                    self.event_name_len += 1;
                } else {
                    self.event_name_overflow = true;
                }
            }
            LineKind::Ignore => {}
        }
    }

    fn finish_line(&mut self) {
        match self.line_kind {
            LineKind::Field if self.used == self.line_start => self.dispatch_event(),
            LineKind::Field => {
                let is_data = &self.storage[self.line_start..self.used] == b"data";
                let is_event = &self.storage[self.line_start..self.used] == b"event";
                self.used = self.line_start;
                if is_data {
                    if self.has_data {
                        self.push_storage(b'\n');
                    }
                    self.has_data = true;
                } else if is_event {
                    self.event = SseEvent::Default;
                }
            }
            LineKind::Data { .. } => self.has_data = true,
            LineKind::Event { .. } => {
                self.event = SseEvent::from_bytes(
                    &self.event_name[..self.event_name_len],
                    self.event_name_overflow,
                );
            }
            LineKind::Ignore => {}
        }
        self.line_start = self.used;
        self.line_kind = LineKind::Field;
        self.reset_event_name();
    }

    fn dispatch_event(&mut self) {
        if self.has_data {
            self.endpoint
                .observe(self.event.as_str(), &self.storage[..self.used]);
        }
        self.used = 0;
        self.line_start = 0;
        self.has_data = false;
        self.event = SseEvent::Default;
    }

    fn push_storage(&mut self, byte: u8) {
        if self.used == self.storage.len() {
            self.mark_overflow();
            return;
        }
        self.storage[self.used] = byte;
        self.used += 1;
    }

    fn reset_event_name(&mut self) {
        self.event_name_len = 0;
        self.event_name_overflow = false;
    }

    fn mark_overflow(&mut self) {
        self.overflowed = true;
        self.endpoint.mark_invalid();
    }

    #[cfg(test)]
    fn retained_buffer_capacity(&self) -> usize {
        self.storage.len() + self.event_name.len() + self.bom_prefix.len()
    }
}

async fn wait_for_signal(signal: &mut tokio::sync::watch::Receiver<bool>) {
    while !*signal.borrow() {
        if signal.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Endpoint;

    use super::{MAX_EVENT_METADATA, SseDecoder};

    #[test]
    fn json_observer_storage_stops_at_limit_and_header_ambiguity_is_unknown() {
        use super::{ResponseObserver, observable_representation};
        use axum::http::{HeaderMap, HeaderValue, StatusCode};
        let mut observer =
            ResponseObserver::new(Endpoint::ChatCompletions, Some("application/json"), false);
        observer.observe(&vec![b' '; MAX_EVENT_METADATA]);
        assert!(!observer.overflowed());
        observer.observe(b"x");
        observer.observe(&vec![b'x'; MAX_EVENT_METADATA]);
        assert!(observer.overflowed());
        assert!(observer.finish().0.is_none());
        if let ResponseObserver::Json { storage, used, .. } = observer {
            assert_eq!(storage.len(), MAX_EVENT_METADATA);
            assert_eq!(used, MAX_EVENT_METADATA);
        } else {
            panic!("JSON observer");
        }
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        assert_eq!(
            observable_representation(StatusCode::OK, &headers),
            Some("application/json")
        );
        headers.insert(
            "content-encoding",
            HeaderValue::from_bytes(b"\xff").unwrap(),
        );
        assert!(observable_representation(StatusCode::OK, &headers).is_none());
        headers.remove("content-encoding");
        headers.insert(
            "content-type",
            HeaderValue::from_bytes(b"application/json;\xff").unwrap(),
        );
        assert!(observable_representation(StatusCode::OK, &headers).is_none());
    }

    #[test]
    fn observer_retained_buffer_capacity_is_bounded_across_fields_and_lines() {
        let mut decoder = SseDecoder::new(Endpoint::ChatCompletions, false);
        decoder.observe(
            &[
                b"data: ".to_vec(),
                vec![b'x'; 70_000],
                b"\ndata: y\n".to_vec(),
                b"event: ".to_vec(),
                vec![b'z'; 60_000],
                b"\n:".to_vec(),
                vec![b'a'; 131_071],
            ]
            .concat(),
        );
        let retained = decoder.retained_buffer_capacity();
        assert!(retained <= MAX_EVENT_METADATA, "retained {retained} bytes");
        assert!(
            !decoder.overflowed,
            "comments and unknown events are harmless"
        );
    }
}

#[cfg(test)]
mod retained_delivery_tests {
    use super::*;
    #[tokio::test]
    async fn completed_eof_producers_leave_bounded_payload_owned_by_each_response() {
        let metrics = Metrics::new();
        let mut responses = Vec::new();
        for _ in 0..20 {
            let (sender, receiver) = mpsc::channel(DELIVERY_ITEMS);
            let (terminal, terminal_receiver) = oneshot::channel();
            responses.push(downstream_body(receiver, terminal_receiver));
            let budget = Arc::new(Semaphore::new(DELIVERY_BYTES));
            assert!(matches!(
                deliver(
                    &sender,
                    &budget,
                    &metrics,
                    &vec![b'x'; DELIVERY_BYTES],
                    Instant::now() + std::time::Duration::from_secs(1)
                )
                .await,
                Delivery::Delivered
            ));
            terminal.send(BodyTerminal::Eof).ok().unwrap();
            drop(sender);
        }
        // This is the production delivery primitive after producer EOF, not a socket/RSS claim.
        let snapshot: serde_json::Value = serde_json::from_slice(&metrics.status_json()).unwrap();
        assert_eq!(snapshot["queued_response_bytes"], 20 * DELIVERY_BYTES);
        assert!(snapshot["queued_response_bytes"].as_u64().unwrap() > 16 * 64 * 1024);
        drop(responses);
        let snapshot: serde_json::Value = serde_json::from_slice(&metrics.status_json()).unwrap();
        assert_eq!(snapshot["queued_response_bytes"], 0);
    }
}
