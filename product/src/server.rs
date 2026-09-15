use std::convert::Infallible;
use std::fmt;
use std::fs::{self, File};
use std::future::Future;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::http::header::CONTENT_LENGTH;
use axum::http::{Request, Response, StatusCode};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
#[cfg(not(windows))]
use tokio::io::Interest;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

#[cfg(feature = "bench-harness")]
use crate::admission::BenchmarkPolicy;
use crate::admission::{Admission, ManualClock};
use crate::config::{Auth, Config, LoadedConfig, MAX_CONCURRENCY, MIN_CONCURRENCY};
use crate::control;
use crate::metrics::Metrics;
use crate::protocol;
use crate::transport::body_budget::{BodyBudget, BodyReadError};
use crate::transport::headers;
use crate::transport::stream::{self, ForwardRequest, HeadError};
use crate::transport::upstream::UpstreamClient;

const HEADER_BYTES: usize = 32 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const CAPACITY_WAIT_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECTION_LIMIT: usize = 128;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SECRET_BYTES: u64 = 8 * 1024;

#[derive(Clone, Copy)]
struct RuntimeLimits {
    request: Duration,
    capacity_wait: Duration,
    shutdown: Duration,
    #[cfg(feature = "bench-harness")]
    policy: BenchmarkPolicy,
}

const PRODUCTION_LIMITS: RuntimeLimits = RuntimeLimits {
    request: REQUEST_TIMEOUT,
    capacity_wait: CAPACITY_WAIT_TIMEOUT,
    shutdown: SHUTDOWN_TIMEOUT,
    #[cfg(feature = "bench-harness")]
    policy: BenchmarkPolicy::Rr,
};

#[derive(Clone)]
struct Secret(Arc<[u8]>);

impl Secret {
    fn new(value: &[u8], label: &'static str) -> Result<Self, StartError> {
        if value.is_empty() {
            return Err(StartError::InvalidCredential(label));
        }
        axum::http::HeaderValue::from_bytes(value)
            .map_err(|_| StartError::InvalidCredential(label))?;
        Ok(Self(Arc::from(value)))
    }

    fn matches_header(&self, headers: &axum::http::HeaderMap, name: &str) -> bool {
        let mut values = headers.get_all(name).iter();
        let matches = values
            .next()
            .is_some_and(|value| constant_time_equal(self.0.as_ref(), value.as_bytes()));
        matches && values.next().is_none()
    }

    fn bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Clone)]
pub struct RuntimeCredentials {
    control_token: Secret,
    upstream_token: Option<Secret>,
    pub(crate) identity: Option<crate::lifecycle::identity::Identity>,
}

impl RuntimeCredentials {
    pub fn new(control_token: &[u8], upstream_token: Option<&[u8]>) -> Result<Self, StartError> {
        let control_token = Secret::new(control_token, "control token")?;
        let upstream_token = upstream_token
            .map(|value| Secret::new(value, "upstream credential"))
            .transpose()?;
        Ok(Self {
            control_token,
            upstream_token,
            identity: None,
        })
    }

    pub fn load(loaded: &LoadedConfig) -> Result<Self, StartError> {
        let control = read_secret_file(&loaded.state_paths.control_token)?;
        let upstream = match &loaded.config.upstream.auth {
            Auth::Env { name, .. } => Some(
                std::env::var_os(name)
                    .ok_or(StartError::MissingEnvironmentCredential)?
                    .into_encoded_bytes(),
            ),
            Auth::Forward | Auth::None => None,
        };
        Self::new(&control, upstream.as_deref())
    }

    fn validate_for(&self, auth: &Auth) -> Result<(), StartError> {
        if let Auth::Env { header, .. } = auth {
            if headers::is_reserved_auth_target(header) {
                return Err(StartError::ReservedAuthenticationHeader);
            }
        }
        match (auth, self.upstream_token.is_some()) {
            (Auth::Env { .. }, false) => Err(StartError::MissingEnvironmentCredential),
            (Auth::Forward | Auth::None, true) => Err(StartError::UnexpectedEnvironmentCredential),
            _ => Ok(()),
        }
    }
}

pub struct GatewayHandle {
    identity: Option<crate::lifecycle::identity::Identity>,
    address: SocketAddr,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<Result<(), StartError>>>,
    metrics: Metrics,
    admission: Admission,
}

impl GatewayHandle {
    pub(crate) fn identity(&self) -> Option<&crate::lifecycle::identity::Identity> {
        self.identity.as_ref()
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn shutdown(mut self) -> Result<(), StartError> {
        let _ = self.stop.send(true);
        self.join().await
    }

    pub async fn run_foreground(mut self) -> Result<(), StartError> {
        let task = self.task.as_mut().expect("gateway task exists");
        tokio::select! {
            joined = task => {
                self.task = None;
                flatten_join(joined)
            }
            signal = shutdown_signal() => {
                signal.map_err(StartError::Signal)?;
                let _ = self.stop.send(true);
                self.join().await
            }
        }
    }

    async fn join(&mut self) -> Result<(), StartError> {
        let Some(task) = self.task.take() else {
            return Ok(());
        };
        flatten_join(task.await)
    }
}

#[derive(Debug)]
pub enum StartError {
    NonLoopbackListen,
    InvalidConcurrency,
    InvalidRoots,
    Bind(io::Error),
    HttpClient(crate::transport::upstream::BuildError),
    InvalidCredential(&'static str),
    InvalidStartupHold,
    InvalidCache,
    MissingEnvironmentCredential,
    UnexpectedEnvironmentCredential,
    ReservedAuthenticationHeader,
    SecretFile { path: PathBuf, reason: &'static str },
    Signal(io::Error),
    ServerTask,
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonLoopbackListen => {
                formatter.write_str("configured listen address must be loopback")
            }
            Self::InvalidRoots => formatter
                .write_str("configured root identities must be unique, valid, and at most 16"),
            Self::InvalidConcurrency => {
                formatter.write_str("configured concurrency must be between 1 and 16")
            }
            Self::Bind(_) => formatter.write_str("failed to bind the configured loopback address"),
            Self::HttpClient(source) => source.fmt(formatter),
            Self::InvalidCredential(label) => {
                write!(
                    formatter,
                    "{label} is empty or not a valid HTTP header value"
                )
            }
            Self::InvalidCache => {
                formatter.write_str("cache ttl_secs must be 1..3600 and max_history must be 1..64")
            }
            Self::InvalidStartupHold => {
                formatter.write_str("startup_hold_secs must be between 0 and 3600")
            }
            Self::MissingEnvironmentCredential => {
                formatter.write_str("configured upstream environment credential is missing")
            }
            Self::UnexpectedEnvironmentCredential => {
                formatter.write_str("upstream credential is only valid with env authentication")
            }
            Self::ReservedAuthenticationHeader => formatter.write_str(
                "configured upstream authentication header is reserved by HTTP transport",
            ),
            Self::SecretFile { path, reason } => write!(
                formatter,
                "cannot use protected credential file {}: {reason}",
                path.display()
            ),
            Self::Signal(_) => formatter.write_str("failed to listen for foreground shutdown"),
            Self::ServerTask => formatter.write_str("gateway server task ended unexpectedly"),
        }
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind(source) | Self::Signal(source) => Some(source),
            Self::HttpClient(source) => Some(source),
            _ => None,
        }
    }
}

struct AppState {
    cache: Option<crate::cache::ExactCache>,
    config: Config,
    credentials: RuntimeCredentials,
    upstream: UpstreamClient,
    bodies: BodyBudget,
    metrics: Metrics,
    admission: Admission,
    prestart_gate: Option<watch::Receiver<bool>>,
    workers: WorkerSpawner,
    stop: watch::Sender<bool>,
    limits: RuntimeLimits,
}

type Worker = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Clone)]
struct WorkerSpawner {
    sender: mpsc::Sender<Worker>,
}

impl WorkerSpawner {
    async fn spawn(&self, worker: Worker) -> Result<(), ()> {
        self.sender.send(worker).await.map_err(|_| ())
    }
}

pub fn validate_start_config(config: &Config) -> Result<(), StartError> {
    if !config.listen.ip().is_loopback() {
        return Err(StartError::NonLoopbackListen);
    }
    if config.cache.is_some_and(|cache| !cache.is_valid()) {
        return Err(StartError::InvalidCache);
    }
    if config.startup_hold_secs > 3600 {
        return Err(StartError::InvalidStartupHold);
    }
    if !(MIN_CONCURRENCY..=MAX_CONCURRENCY).contains(&config.concurrency) {
        return Err(StartError::InvalidConcurrency);
    }
    crate::config::registered_root_ids(&config.roots).map_err(|_| StartError::InvalidRoots)?;
    Ok(())
}

/// Builds and drops the same upstream client used by a worker. This performs only
/// local proxy/TLS preparation and must run before a restart stops the old worker.
pub(crate) fn preflight_upstream(config: &Config) -> Result<(), StartError> {
    build_upstream(config, PRODUCTION_LIMITS.request).map(drop)
}

fn build_upstream(config: &Config, timeout: Duration) -> Result<UpstreamClient, StartError> {
    UpstreamClient::new(timeout, &config.upstream).map_err(StartError::HttpClient)
}

pub async fn spawn(
    config: Config,
    credentials: RuntimeCredentials,
) -> Result<GatewayHandle, StartError> {
    spawn_inner(config, credentials, PRODUCTION_LIMITS, None, None).await
}

/// Benchmark-only policy selection; config, transport and deadlines remain production values.
#[cfg(feature = "bench-harness")]
pub async fn spawn_benchmark(
    config: Config,
    credentials: RuntimeCredentials,
    policy: BenchmarkPolicy,
) -> Result<GatewayHandle, StartError> {
    spawn_inner(
        config,
        credentials,
        RuntimeLimits {
            policy,
            ..PRODUCTION_LIMITS
        },
        None,
        None,
    )
    .await
}

async fn spawn_inner(
    config: Config,
    credentials: RuntimeCredentials,
    limits: RuntimeLimits,
    clock: Option<ManualClock>,
    prestart_gate: Option<watch::Receiver<bool>>,
) -> Result<GatewayHandle, StartError> {
    validate_start_config(&config)?;
    credentials.validate_for(&config.upstream.auth)?;
    let listener = TcpListener::bind(config.listen)
        .await
        .map_err(StartError::Bind)?;
    let address = listener.local_addr().map_err(StartError::Bind)?;
    let mut config = config;
    config.listen = address;
    let mut credentials = credentials;
    if let Some(identity) = &mut credentials.identity {
        identity.address = address;
    }
    let handle_identity = credentials.identity.clone();
    let upstream = build_upstream(&config, limits.request)?;
    let (stop, stop_rx) = watch::channel(false);
    let (worker_sender, worker_receiver) = mpsc::channel(16);
    let (force_workers, force_workers_rx) = tokio::sync::oneshot::channel();
    let worker_supervisor = tokio::spawn(supervise_workers(
        worker_receiver,
        stop_rx.clone(),
        force_workers_rx,
    ));
    let admission = Admission::new(&config, clock.clone());
    #[cfg(feature = "bench-harness")]
    let admission = match limits.policy {
        BenchmarkPolicy::Rr => admission,
        policy => Admission::benchmark(&config, clock, policy),
    };
    let handle_admission = admission.clone();
    let state = Arc::new(AppState {
        cache: config.cache.map(crate::cache::ExactCache::new),
        config,
        credentials,
        upstream,
        bodies: BodyBudget::new(),
        metrics: Metrics::new(),
        admission,
        prestart_gate,
        workers: WorkerSpawner {
            sender: worker_sender,
        },
        stop: stop.clone(),
        limits,
    });
    let handle_metrics = state.metrics.clone();
    let task = tokio::spawn(serve(
        listener,
        state,
        stop_rx,
        worker_supervisor,
        force_workers,
    ));
    Ok(GatewayHandle {
        identity: handle_identity,
        address,
        stop,
        task: Some(task),
        metrics: handle_metrics,
        admission: handle_admission,
    })
}

#[doc(hidden)]
pub mod testing {
    use std::time::Duration;

    use crate::config::Config;

    use super::{GatewayHandle, RuntimeCredentials, RuntimeLimits, StartError, spawn_inner};

    pub async fn spawn_with_timeouts(
        config: Config,
        credentials: RuntimeCredentials,
        request: Duration,
        shutdown: Duration,
    ) -> Result<GatewayHandle, StartError> {
        spawn_inner(
            config,
            credentials,
            RuntimeLimits {
                request,
                capacity_wait: request,
                shutdown,
                #[cfg(feature = "bench-harness")]
                policy: super::BenchmarkPolicy::Rr,
            },
            None,
            None,
        )
        .await
    }

    pub async fn spawn_with_clock(
        config: Config,
        credentials: RuntimeCredentials,
        clock: crate::admission::ManualClock,
        prestart_gate: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<GatewayHandle, StartError> {
        spawn_inner(
            config,
            credentials,
            super::PRODUCTION_LIMITS,
            Some(clock),
            prestart_gate,
        )
        .await
    }

    pub fn quota_snapshot(gateway: &GatewayHandle) -> crate::admission::quota::Snapshot {
        gateway.admission.snapshot()
    }

    pub async fn shutdown_with_status(mut gateway: GatewayHandle) -> Result<Vec<u8>, StartError> {
        let _ = gateway.stop.send(true);
        gateway.join().await?;
        Ok(gateway.metrics.status_json())
    }
}

async fn serve(
    listener: TcpListener,
    state: Arc<AppState>,
    mut stop: watch::Receiver<bool>,
    mut worker_supervisor: JoinHandle<()>,
    force_workers: tokio::sync::oneshot::Sender<()>,
) -> Result<(), StartError> {
    let connections = Arc::new(Semaphore::new(CONNECTION_LIMIT));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() { break; }
            }
            completed = tasks.join_next(), if !tasks.is_empty() => {
                let _ = completed;
            }
            accepted = listener.accept() => {
                let (socket, _) = accepted.map_err(StartError::Bind)?;
                let Ok(permit) = connections.clone().try_acquire_owned() else {
                    continue;
                };
                let state = state.clone();
                let mut connection_stop = state.stop.subscribe();
                tasks.spawn(async move {
                    let _permit = permit;
                    let (disconnect, disconnect_rx) = watch::channel(false);
                    let standard = socket.into_std().expect("accepted socket conversion");
                    let monitor_standard = standard.try_clone().expect("accepted socket clone");
                    let socket = tokio::net::TcpStream::from_std(standard)
                        .expect("accepted async socket conversion");
                    let service = service_fn(move |request| {
                        handle(request, state.clone(), disconnect_rx.clone())
                    });
                    let mut builder = http1::Builder::new();
                    builder
                        .timer(TokioTimer::new())
                        .header_read_timeout(HEADER_TIMEOUT)
                        .max_buf_size(HEADER_BYTES)
                        .max_headers(128)
                        .half_close(true);
                    let io = DisconnectAwareIo {
                        inner: socket,
                        disconnect: disconnect.clone(),
                    };
                    let connection = builder.serve_connection(TokioIo::new(io), service);
                    let reset_monitor = monitor_reset(monitor_standard, disconnect.clone());
                    tokio::pin!(connection);
                    tokio::pin!(reset_monitor);
                    tokio::select! {
                        _ = &mut connection => {}
                        () = &mut reset_monitor => {}
                        () = wait_for_stop(&mut connection_stop) => {
                            connection.as_mut().graceful_shutdown();
                            let _ = connection.await;
                        }
                    }
                    let _ = disconnect.send(true);
                });
            }
        }
    }
    // Keep authenticated control available during the bounded drain. New data
    // requests are rejected by handle_request; these short connections do not
    // extend the drain deadline or hold application worker resources.
    let mut controls = JoinSet::new();
    let mut supervisor_done = false;
    let drained = tokio::time::timeout(state.limits.shutdown, async {
        loop {
            if tasks.is_empty() && supervisor_done { break; }
            tokio::select! {
                _ = tasks.join_next(), if !tasks.is_empty() => {},
                _ = &mut worker_supervisor, if !supervisor_done => { supervisor_done = true; },
                _ = controls.join_next(), if !controls.is_empty() => {},
                accepted = listener.accept() => {
                    let Ok((socket, _)) = accepted else { break; };
                    let Ok(permit) = connections.clone().try_acquire_owned() else { continue; };
                    let state = state.clone();
                    controls.spawn(async move {
                        let _permit = permit;
                        let (_disconnect, disconnect_rx) = watch::channel(false);
                        let service = service_fn(move |request| handle(request, state.clone(), disconnect_rx.clone()));
                        let mut builder = http1::Builder::new();
                        builder
                            .keep_alive(false)
                            .half_close(true)
                            .timer(TokioTimer::new())
                            .header_read_timeout(HEADER_TIMEOUT)
                            .max_buf_size(HEADER_BYTES)
                            .max_headers(128);
                        let _ = builder.serve_connection(TokioIo::new(socket), service).await;
                    });
                }
            }
        }
    }).await;
    drop(listener);
    controls.abort_all();
    while controls.join_next().await.is_some() {}
    if drained.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        let _ = force_workers.send(());
        if !supervisor_done {
            let _ = worker_supervisor.await;
        }
    }
    Ok(())
}

#[cfg(windows)]
async fn monitor_reset(socket: std::net::TcpStream, disconnect: watch::Sender<bool>) {
    if crate::transport::windows_disconnect::reset(socket)
        .await
        .unwrap_or(true)
    {
        let _ = disconnect.send(true);
    }
    std::future::pending::<()>().await;
}

#[cfg(not(windows))]
async fn monitor_reset(socket: std::net::TcpStream, disconnect: watch::Sender<bool>) {
    let socket =
        tokio::net::TcpStream::from_std(socket).expect("accepted monitor socket conversion");
    let result: io::Result<()> = socket
        .async_io(Interest::ERROR, || {
            match socket2::SockRef::from(&socket).take_error() {
                Ok(Some(error)) => Err(error),
                Ok(None) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                Err(error) => Err(error),
            }
        })
        .await;
    if result.is_err() {
        let _ = disconnect.send(true);
        std::future::pending::<()>().await;
    } else {
        // The operation above returns only errors, but keep this monitor alive
        // if a platform reports an unexpected success.
        std::future::pending::<()>().await;
    }
}

async fn supervise_workers(
    mut receiver: mpsc::Receiver<Worker>,
    mut stop: watch::Receiver<bool>,
    mut force: tokio::sync::oneshot::Receiver<()>,
) {
    let mut workers = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = &mut force => {
                receiver.close();
                workers.abort_all();
                while workers.join_next().await.is_some() {}
                return;
            }
            () = wait_for_stop(&mut stop) => break,
            completed = workers.join_next(), if !workers.is_empty() => {
                let _ = completed;
            }
            worker = receiver.recv() => match worker {
                Some(worker) => { workers.spawn(worker); }
                None => break,
            }
        }
    }
    receiver.close();
    while let Ok(worker) = receiver.try_recv() {
        workers.spawn(worker);
    }
    while !workers.is_empty() {
        tokio::select! {
            biased;
            _ = &mut force => {
                workers.abort_all();
                while workers.join_next().await.is_some() {}
                return;
            }
            completed = workers.join_next() => {
                let _ = completed;
            }
        }
    }
}

async fn wait_for_stop(stop: &mut watch::Receiver<bool>) {
    while !*stop.borrow() {
        if stop.changed().await.is_err() {
            break;
        }
    }
}

async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
    downstream_disconnect: watch::Receiver<bool>,
) -> Result<Response<Body>, Infallible> {
    state.metrics.record_request();
    Ok(handle_request(request, &state, downstream_disconnect).await)
}

async fn handle_request(
    request: Request<Incoming>,
    state: &AppState,
    mut downstream_disconnect: watch::Receiver<bool>,
) -> Response<Body> {
    if headers::stored_size(request.headers()) > HEADER_BYTES {
        return protocol::error(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "headers_too_large",
        );
    }
    let path = request.uri().path();
    if path.starts_with("/_llmgw/") {
        return control::handle(
            request.method(),
            path,
            request.headers(),
            state
                .credentials
                .control_token
                .matches_header(request.headers(), control::CONTROL_TOKEN_HEADER),
            state
                .credentials
                .identity
                .as_ref()
                .map(|identity| identity.nonce.as_str()),
            || {
                let mut bytes = state
                    .metrics
                    .status_with_admission(&state.admission, state.bodies.stored_bytes());
                bytes.pop();
                bytes.extend_from_slice(b",\"exact_cache\":");
                serde_json::to_writer(
                    &mut bytes,
                    &crate::cache::ExactCache::snapshot(state.cache.as_ref()),
                )
                .expect("cache status JSON");
                bytes.extend_from_slice(if *state.stop.borrow() {
                    b",\"status\":\"ok\",\"state\":\"draining\",\"identity\":"
                } else {
                    b",\"status\":\"ok\",\"state\":\"running\",\"identity\":"
                });
                serde_json::to_writer(&mut bytes, &state.credentials.identity)
                    .expect("bounded identity JSON");
                bytes.push(b'}');
                bytes
            },
            &state.stop,
        );
    }
    if *state.stop.borrow() {
        return protocol::error(StatusCode::SERVICE_UNAVAILABLE, "gateway_stopping");
    }
    let route = match protocol::route(&state.config, request.method(), path) {
        Ok(route) => route,
        Err(error) => return error.into_response(),
    };
    if let Err((status, code)) = headers::validate_local_request(
        request.headers(),
        request.method(),
        state.config.listen.port(),
    ) {
        return protocol::error(status, code);
    }
    if headers::requests_upgrade(request.headers()) {
        return protocol::error(StatusCode::BAD_REQUEST, "unsupported_upgrade");
    }
    let upstream_url = match protocol::upstream_url(
        &state.config.upstream.api_base,
        &route,
        request.uri().query(),
    ) {
        Ok(url) => url,
        Err(error) => return error.into_response(),
    };
    let content_length = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if content_length.is_some_and(|length| length > BodyBudget::PER_REQUEST_LIMIT) {
        return protocol::error(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large");
    }
    let deadline = tokio::time::Instant::now() + state.limits.request;
    let (parts, body) = request.into_parts();
    let mut body_stop = state.stop.subscribe();
    let body_result = tokio::select! {
        biased;
        () = wait_for_stop(&mut body_stop) => {
            return protocol::error(StatusCode::SERVICE_UNAVAILABLE, "gateway_stopping");
        }
        () = wait_for_stop(&mut downstream_disconnect) => {
            return protocol::error(StatusCode::BAD_REQUEST, "downstream_disconnected");
        }
        () = tokio::time::sleep_until(deadline) => {
            return protocol::error(StatusCode::GATEWAY_TIMEOUT, "gateway_request_deadline");
        }
        result = tokio::time::timeout(BODY_TIMEOUT, state.bodies.read(body)) => result,
    };
    let body = match body_result {
        Ok(Ok(body)) => body,
        Ok(Err(BodyReadError::TooLarge)) => {
            return protocol::error(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large");
        }
        Ok(Err(BodyReadError::MemoryFull)) => {
            return protocol::error(StatusCode::TOO_MANY_REQUESTS, "gateway_memory_full");
        }
        Ok(Err(BodyReadError::Read)) => {
            return protocol::error(StatusCode::BAD_REQUEST, "body_read_failed");
        }
        Err(_) => return protocol::error(StatusCode::REQUEST_TIMEOUT, "body_read_timeout"),
    };
    let cost = match protocol::inspect_body(&state.config, &route, body.bytes()) {
        Ok(cost) => cost,
        Err(error) => return error.into_response(),
    };
    let forbids_cache_reuse = state.cache.is_some() && crate::cache::forbids_reuse(&parts.headers);
    let mut request_headers = parts.headers;
    headers::prepare_request(
        &mut request_headers,
        &state.config.upstream.auth,
        state.credentials.upstream_token.as_ref().map(Secret::bytes),
    );
    let cache = state.cache.as_ref().and_then(|cache| {
        cache.request(
            route.root_index,
            &upstream_url,
            &request_headers,
            route.endpoint,
            body.bytes(),
            forbids_cache_reuse,
        )
    });
    if let Some(hit) = cache.as_ref().and_then(crate::cache::Pending::lookup) {
        return hit;
    }
    let queue_deadline = (tokio::time::Instant::now() + state.limits.capacity_wait).min(deadline);
    let mut stop = state.stop.subscribe();
    let admission_hold = tokio::select! {
        biased;
        () = wait_for_stop(&mut stop) => {
            return protocol::error(StatusCode::SERVICE_UNAVAILABLE, "gateway_stopping");
        }
        () = wait_for_stop(&mut downstream_disconnect) => {
            return protocol::error(StatusCode::BAD_REQUEST, "downstream_disconnected");
        }
        () = tokio::time::sleep_until(queue_deadline) => {
            return protocol::error(StatusCode::GATEWAY_TIMEOUT, "gateway_queue_deadline");
        }
        permit = state.admission.acquire(route.root_index, cost, route.endpoint, state.limits.capacity_wait.min(deadline.saturating_duration_since(tokio::time::Instant::now()))) => match permit {
            Ok(permit) => permit,
            Err(crate::admission::AcquireError::EstimateExceedsBudget) => return protocol::error(StatusCode::BAD_REQUEST, "estimate_exceeds_budget"),
            Err(crate::admission::AcquireError::QueueFull) => return protocol::error(StatusCode::TOO_MANY_REQUESTS, "gateway_queue_full"),
            Err(crate::admission::AcquireError::Deadline) => return protocol::error(StatusCode::GATEWAY_TIMEOUT, "gateway_queue_deadline"),
            Err(crate::admission::AcquireError::InvalidRoot) => return protocol::error(StatusCode::NOT_FOUND, "route_not_found"),
        }
    };
    let (head_sender, head_receiver) = tokio::sync::oneshot::channel();
    let worker = stream::forward(ForwardRequest {
        cache,
        client: state.upstream.clone(),
        method: parts.method,
        url: upstream_url,
        headers: request_headers,
        body,
        endpoint: route.endpoint,
        cancel_policy: state.config.cancel_policy,
        retry_transient_429: state.config.retry_transient_429,
        deadline,
        admission_hold,
        prestart_gate: state.prestart_gate.clone(),
        stop: state.stop.subscribe(),
        metrics: state.metrics.clone(),
        head: head_sender,
        downstream_disconnect,
    });
    let spawned = tokio::select! {
        biased;
        () = wait_for_stop(&mut stop) => false,
        result = state.workers.spawn(Box::pin(worker)) => result.is_ok(),
    };
    if !spawned {
        return protocol::error(StatusCode::SERVICE_UNAVAILABLE, "gateway_stopping");
    }
    match tokio::time::timeout_at(deadline, head_receiver).await {
        Ok(Ok(Ok(response))) => response,
        Ok(Ok(Err(HeadError::Deadline))) | Err(_) => {
            protocol::error(StatusCode::GATEWAY_TIMEOUT, "upstream_deadline")
        }
        Ok(Ok(Err(HeadError::Admission(error)))) => match error {
            crate::admission::AcquireError::QueueFull => {
                protocol::error(StatusCode::TOO_MANY_REQUESTS, "gateway_queue_full")
            }
            crate::admission::AcquireError::Deadline => {
                protocol::error(StatusCode::GATEWAY_TIMEOUT, "gateway_queue_deadline")
            }
            crate::admission::AcquireError::EstimateExceedsBudget => {
                protocol::error(StatusCode::BAD_REQUEST, "estimate_exceeds_budget")
            }
            crate::admission::AcquireError::InvalidRoot => {
                protocol::error(StatusCode::NOT_FOUND, "route_not_found")
            }
        },
        Ok(Ok(Err(HeadError::Upstream))) | Ok(Err(_)) => {
            protocol::error(StatusCode::BAD_GATEWAY, "upstream_transport_error")
        }
    }
}

struct DisconnectAwareIo {
    inner: tokio::net::TcpStream,
    disconnect: watch::Sender<bool>,
}

impl AsyncRead for DisconnectAwareIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_read(context, buffer);
        if matches!(result, Poll::Ready(Err(_))) {
            let _ = self.disconnect.send(true);
        }
        result
    }
}

impl AsyncWrite for DisconnectAwareIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(context, buffer);
        if matches!(result, Poll::Ready(Err(_))) {
            let _ = self.disconnect.send(true);
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(context);
        if matches!(result, Poll::Ready(Err(_))) {
            let _ = self.disconnect.send(true);
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write_vectored(context, buffers);
        if matches!(result, Poll::Ready(Err(_))) {
            let _ = self.disconnect.send(true);
        }
        result
    }
}

fn constant_time_equal(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    let length = expected.len().max(actual.len());
    for index in 0..length {
        difference |= usize::from(
            expected.get(index).copied().unwrap_or(0) ^ actual.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

async fn shutdown_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { signal = tokio::signal::ctrl_c() => signal, _ = terminate.recv() => Ok(()) }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}

fn flatten_join(
    joined: Result<Result<(), StartError>, tokio::task::JoinError>,
) -> Result<(), StartError> {
    joined.map_err(|_| StartError::ServerTask)?
}

pub(crate) fn read_secret_file(path: &Path) -> Result<Vec<u8>, StartError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StartError::SecretFile {
        path: path.to_owned(),
        reason: "file is missing or unreadable",
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(StartError::SecretFile {
            path: path.to_owned(),
            reason: "path must be a regular file and not a symbolic link",
        });
    }
    let mut file = open_secret(path)?;
    let opened = file.metadata().map_err(|_| StartError::SecretFile {
        path: path.to_owned(),
        reason: "cannot inspect opened file",
    })?;
    if !opened.is_file() {
        return Err(StartError::SecretFile {
            path: path.to_owned(),
            reason: "opened path is not a regular file",
        });
    }
    let mut value = Vec::new();
    file.by_ref()
        .take(MAX_SECRET_BYTES + 1)
        .read_to_end(&mut value)
        .map_err(|_| StartError::SecretFile {
            path: path.to_owned(),
            reason: "cannot read file",
        })?;
    if value.len() as u64 > MAX_SECRET_BYTES {
        return Err(StartError::SecretFile {
            path: path.to_owned(),
            reason: "file exceeds the credential size limit",
        });
    }
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        value.pop();
    }
    Ok(value)
}

fn open_secret(path: &Path) -> Result<File, StartError> {
    crate::lifecycle::platform::read(path).map_err(|_| StartError::SecretFile {
        path: path.to_owned(),
        reason: "cannot open a private, owned regular file without links",
    })
}
