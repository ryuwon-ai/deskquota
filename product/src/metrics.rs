use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::{ObservedUsage, ResponseError};

#[derive(Clone)]
pub struct Metrics {
    inner: Arc<Inner>,
}

struct Inner {
    requests: AtomicU64,
    upstream_attempts: AtomicU64,
    active: AtomicU64,
    draining: AtomicU64,
    terminal_total: AtomicU64,
    terminal_body_eof: AtomicU64,
    terminal_upstream_error: AtomicU64,
    terminal_deadline: AtomicU64,
    terminal_downstream_close: AtomicU64,
    terminal_shutdown: AtomicU64,
    terminal_admission_rejected: AtomicU64,
    usage_known: AtomicU64,
    usage_unknown: AtomicU64,
    observer_overflow: AtomicU64,
    first_body_byte: AtomicU64,
    first_observed_output_delta: AtomicU64,
    terminal_marker: AtomicU64,
    response_body_eof: AtomicU64,
    response_error_server: AtomicU64,
    response_error_rate_limit: AtomicU64,
    response_error_client: AtomicU64,
    response_error_unknown: AtomicU64,
    queued_response_bytes: AtomicU64,
    max_queued_response_bytes: AtomicU64,
    observed_input_tokens: AtomicU64,
    observed_output_tokens: AtomicU64,
    observed_cache_creation_tokens: AtomicU64,
    observed_cache_read_tokens: AtomicU64,
}

#[derive(Clone, Copy)]
pub enum TerminalReason {
    BodyEof,
    UpstreamError,
    Deadline,
    DownstreamClose,
    Shutdown,
    AdmissionRejected,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                requests: AtomicU64::new(0),
                upstream_attempts: AtomicU64::new(0),
                active: AtomicU64::new(0),
                draining: AtomicU64::new(0),
                terminal_total: AtomicU64::new(0),
                terminal_body_eof: AtomicU64::new(0),
                terminal_upstream_error: AtomicU64::new(0),
                terminal_deadline: AtomicU64::new(0),
                terminal_downstream_close: AtomicU64::new(0),
                terminal_shutdown: AtomicU64::new(0),
                terminal_admission_rejected: AtomicU64::new(0),
                usage_known: AtomicU64::new(0),
                usage_unknown: AtomicU64::new(0),
                observer_overflow: AtomicU64::new(0),
                first_body_byte: AtomicU64::new(0),
                first_observed_output_delta: AtomicU64::new(0),
                terminal_marker: AtomicU64::new(0),
                response_body_eof: AtomicU64::new(0),
                response_error_server: AtomicU64::new(0),
                response_error_rate_limit: AtomicU64::new(0),
                response_error_client: AtomicU64::new(0),
                response_error_unknown: AtomicU64::new(0),
                queued_response_bytes: AtomicU64::new(0),
                max_queued_response_bytes: AtomicU64::new(0),
                observed_input_tokens: AtomicU64::new(0),
                observed_output_tokens: AtomicU64::new(0),
                observed_cache_creation_tokens: AtomicU64::new(0),
                observed_cache_read_tokens: AtomicU64::new(0),
            }),
        }
    }

    pub fn record_request(&self) {
        self.inner.requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_upstream_attempt(&self) {
        self.inner.upstream_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_started(&self) {
        self.inner.active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn draining_started(&self) {
        self.inner.draining.fetch_add(1, Ordering::Relaxed);
    }

    pub fn draining_finished(&self) {
        self.inner.draining.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_first_body_byte(&self) {
        self.inner.first_body_byte.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_first_observed_output_delta(&self) {
        self.inner
            .first_observed_output_delta
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_terminal_marker(&self) {
        self.inner.terminal_marker.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_response_body_eof(&self) {
        self.inner.response_body_eof.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_response_error(&self, error: ResponseError) {
        match error {
            ResponseError::Server => &self.inner.response_error_server,
            ResponseError::RateLimit => &self.inner.response_error_rate_limit,
            ResponseError::Client => &self.inner.response_error_client,
            ResponseError::Unknown => &self.inner.response_error_unknown,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_observer_overflow(&self) {
        self.inner.observer_overflow.fetch_add(1, Ordering::Relaxed);
    }

    pub fn queued_bytes_added(&self, bytes: usize) {
        let bytes = u64::try_from(bytes).expect("response chunk length fits u64");
        let current = self
            .inner
            .queued_response_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            + bytes;
        self.inner
            .max_queued_response_bytes
            .fetch_max(current, Ordering::Relaxed);
    }

    pub fn queued_bytes_removed(&self, bytes: usize) {
        let bytes = u64::try_from(bytes).expect("response chunk length fits u64");
        self.inner
            .queued_response_bytes
            .fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn worker_finished(&self, reason: TerminalReason, usage: Option<ObservedUsage>) {
        self.inner.active.fetch_sub(1, Ordering::Relaxed);
        self.inner.terminal_total.fetch_add(1, Ordering::Relaxed);
        match reason {
            TerminalReason::BodyEof => &self.inner.terminal_body_eof,
            TerminalReason::UpstreamError => &self.inner.terminal_upstream_error,
            TerminalReason::Deadline => &self.inner.terminal_deadline,
            TerminalReason::DownstreamClose => &self.inner.terminal_downstream_close,
            TerminalReason::Shutdown => &self.inner.terminal_shutdown,
            TerminalReason::AdmissionRejected => &self.inner.terminal_admission_rejected,
        }
        .fetch_add(1, Ordering::Relaxed);
        if let Some(usage) = usage {
            self.inner.usage_known.fetch_add(1, Ordering::Relaxed);
            self.inner
                .observed_input_tokens
                .fetch_add(usage.input_tokens, Ordering::Relaxed);
            self.inner
                .observed_output_tokens
                .fetch_add(usage.output_tokens, Ordering::Relaxed);
            self.inner
                .observed_cache_creation_tokens
                .fetch_add(usage.cache_creation_input_tokens, Ordering::Relaxed);
            self.inner
                .observed_cache_read_tokens
                .fetch_add(usage.cache_read_input_tokens, Ordering::Relaxed);
        } else {
            self.inner.usage_unknown.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn status_with_admission(
        &self,
        admission: &crate::admission::Admission,
        stored_request_bytes: usize,
    ) -> Vec<u8> {
        let mut status = self.status_json();
        status.pop(); // The bounded metrics object always ends with its closing brace.
        status.extend_from_slice(
            format!(",\"stored_request_bytes\":{stored_request_bytes},\"admission\":").as_bytes(),
        );
        serde_json::to_writer(&mut status, &admission.status()).expect("bounded admission JSON");
        status.push(b'}');
        status
    }

    pub fn status_json(&self) -> Vec<u8> {
        let load = |value: &AtomicU64| value.load(Ordering::Relaxed);
        format!(
            concat!(
                "{{\"requests\":{},\"upstream_attempts\":{},\"active\":{},\"draining\":{},",
                "\"terminal_total\":{},\"terminal_body_eof\":{},\"terminal_upstream_error\":{},",
                "\"terminal_deadline\":{},\"terminal_downstream_close\":{},\"terminal_shutdown\":{},\"terminal_admission_rejected\":{},",
                "\"usage_known\":{},\"usage_unknown\":{},\"observer_overflow\":{},",
                "\"first_body_byte\":{},\"first_observed_output_delta\":{},\"terminal_marker\":{},",
                "\"response_body_eof\":{},\"queued_response_bytes\":{},\"max_queued_response_bytes\":{},",
                "\"observed_input_tokens\":{},\"observed_output_tokens\":{},",
                "\"observed_cache_creation_tokens\":{},\"observed_cache_read_tokens\":{},",
                "\"response_errors\":{{\"server\":{},\"rate_limit\":{},\"client\":{},\"unknown\":{}}}}}"
            ),
            load(&self.inner.requests), load(&self.inner.upstream_attempts),
            load(&self.inner.active), load(&self.inner.draining),
            load(&self.inner.terminal_total), load(&self.inner.terminal_body_eof),
            load(&self.inner.terminal_upstream_error), load(&self.inner.terminal_deadline),
            load(&self.inner.terminal_downstream_close), load(&self.inner.terminal_shutdown),
            load(&self.inner.terminal_admission_rejected),
            load(&self.inner.usage_known), load(&self.inner.usage_unknown),
            load(&self.inner.observer_overflow), load(&self.inner.first_body_byte),
            load(&self.inner.first_observed_output_delta), load(&self.inner.terminal_marker),
            load(&self.inner.response_body_eof), load(&self.inner.queued_response_bytes),
            load(&self.inner.max_queued_response_bytes), load(&self.inner.observed_input_tokens),
            load(&self.inner.observed_output_tokens),
            load(&self.inner.observed_cache_creation_tokens),
            load(&self.inner.observed_cache_read_tokens),
            load(&self.inner.response_error_server), load(&self.inner.response_error_rate_limit),
            load(&self.inner.response_error_client), load(&self.inner.response_error_unknown),
        )
        .into_bytes()
    }
}
