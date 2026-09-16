#![allow(dead_code)]

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};

struct Gate {
    open: AtomicBool,
    notify: Notify,
}

impl Gate {
    fn new() -> Self {
        Self {
            open: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn open(&self) {
        self.open.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            if self.open.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturedRequest {
    pub target: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

impl CapturedRequest {
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}

pub struct UpstreamFixture {
    address: SocketAddr,
    attempts: Arc<AtomicUsize>,
    captures: Arc<Mutex<Vec<CapturedRequest>>>,
    notify: Arc<Notify>,
    stop: watch::Sender<bool>,
}

impl UpstreamFixture {
    pub async fn start(response: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind synthetic upstream");
        let address = listener.local_addr().expect("synthetic upstream address");
        let attempts = Arc::new(AtomicUsize::new(0));
        let captures = Arc::new(Mutex::new(Vec::new()));
        let notify = Arc::new(Notify::new());
        let (stop, mut stop_rx) = watch::channel(false);
        let task_attempts = attempts.clone();
        let task_captures = captures.clone();
        let task_notify = notify.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    changed = stop_rx.changed() => {
                        if changed.is_err() || *stop_rx.borrow() { break; }
                    }
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break; };
                        let response = response.clone();
                        let attempts = task_attempts.clone();
                        let captures = task_captures.clone();
                        let notify = task_notify.clone();
                        tokio::spawn(async move {
                            if let Ok(capture) = read_request(&mut socket).await {
                                attempts.fetch_add(1, Ordering::SeqCst);
                                captures.lock().expect("captures lock").push(capture);
                                notify.notify_waiters();
                                let _ = socket.write_all(&response).await;
                                let _ = socket.shutdown().await;
                            }
                        });
                    }
                }
            }
        });
        Self {
            address,
            attempts,
            captures,
            notify,
            stop,
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    pub async fn captures(&self, expected: usize) -> Vec<CapturedRequest> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let notified = self.notify.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let captures = self.captures.lock().expect("captures lock").clone();
                if captures.len() >= expected {
                    return captures;
                }
                notified.await;
            }
        })
        .await
        .expect("expected upstream request captures")
    }

    pub async fn capture(&self) -> CapturedRequest {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(capture) = self
                    .captures
                    .lock()
                    .expect("captures lock")
                    .first()
                    .cloned()
                {
                    return capture;
                }
                self.notify.notified().await;
            }
        })
        .await
        .expect("upstream request within timeout")
    }
}

impl Drop for UpstreamFixture {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

pub struct GatedUpstreamFixture {
    address: SocketAddr,
    attempts: Arc<AtomicUsize>,
    first_gate: Arc<Gate>,
    final_gate: Arc<Gate>,
    eof_gate: Arc<Gate>,
    first_sent: Arc<AtomicBool>,
    first_written: Arc<Notify>,
    final_sent: Arc<AtomicBool>,
    final_written: Arc<Notify>,
    body_eof: Arc<AtomicBool>,
    disconnected: Arc<AtomicBool>,
    disconnected_notify: Arc<Notify>,
    stop: watch::Sender<bool>,
}

impl GatedUpstreamFixture {
    pub async fn start(first_chunks: Vec<Vec<u8>>, final_chunks: Vec<Vec<u8>>) -> Self {
        Self::start_inner(
            first_chunks,
            final_chunks,
            false,
            false,
            "text/event-stream",
        )
        .await
    }

    pub async fn start_before_headers(
        first_chunks: Vec<Vec<u8>>,
        final_chunks: Vec<Vec<u8>>,
    ) -> Self {
        Self::start_inner(first_chunks, final_chunks, true, false, "text/event-stream").await
    }

    pub async fn start_with_body_error(
        first_chunks: Vec<Vec<u8>>,
        final_chunks: Vec<Vec<u8>>,
    ) -> Self {
        Self::start_inner(first_chunks, final_chunks, false, true, "text/event-stream").await
    }

    pub async fn start_json(
        first_chunks: Vec<Vec<u8>>,
        final_chunks: Vec<Vec<u8>>,
        fail_body: bool,
    ) -> Self {
        Self::start_inner(
            first_chunks,
            final_chunks,
            false,
            fail_body,
            "application/json",
        )
        .await
    }

    async fn start_inner(
        first_chunks: Vec<Vec<u8>>,
        final_chunks: Vec<Vec<u8>>,
        gate_before_headers: bool,
        fail_body: bool,
        media: &'static str,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gated synthetic upstream");
        let address = listener.local_addr().expect("gated upstream address");
        let attempts = Arc::new(AtomicUsize::new(0));
        let first_gate = Arc::new(Gate::new());
        let final_gate = Arc::new(Gate::new());
        let eof_gate = Arc::new(Gate::new());
        let first_sent = Arc::new(AtomicBool::new(false));
        let first_written = Arc::new(Notify::new());
        let final_sent = Arc::new(AtomicBool::new(false));
        let final_written = Arc::new(Notify::new());
        let body_eof = Arc::new(AtomicBool::new(false));
        let disconnected = Arc::new(AtomicBool::new(false));
        let disconnected_notify = Arc::new(Notify::new());
        let (stop, mut stop_rx) = watch::channel(false);
        let task_attempts = attempts.clone();
        let task_first_gate = first_gate.clone();
        let task_final_gate = final_gate.clone();
        let task_eof_gate = eof_gate.clone();
        let task_first_sent = first_sent.clone();
        let task_first_written = first_written.clone();
        let task_final_sent = final_sent.clone();
        let task_final_written = final_written.clone();
        let task_body_eof = body_eof.clone();
        let task_disconnected = disconnected.clone();
        let task_disconnected_notify = disconnected_notify.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    changed = stop_rx.changed() => {
                        if changed.is_err() || *stop_rx.borrow() { break; }
                    }
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break; };
                        let attempts = task_attempts.clone();
                        let first_gate = task_first_gate.clone();
                        let final_gate = task_final_gate.clone();
                        let eof_gate = task_eof_gate.clone();
                        let first_sent = task_first_sent.clone();
                        let first_written = task_first_written.clone();
                        let final_sent = task_final_sent.clone();
                        let final_written = task_final_written.clone();
                        let body_eof = task_body_eof.clone();
                        let disconnected = task_disconnected.clone();
                        let disconnected_notify = task_disconnected_notify.clone();
                        let first_chunks = first_chunks.clone();
                        let final_chunks = final_chunks.clone();
                        tokio::spawn(async move {
                            let mut socket = socket;
                            if read_request(&mut socket).await.is_err() {
                                return;
                            }
                            attempts.fetch_add(1, Ordering::SeqCst);
                            let (mut reader, mut writer) = socket.into_split();
                            if gate_before_headers
                                && wait_gate_or_disconnect(&first_gate, &mut reader).await
                            {
                                mark_disconnected(&disconnected, &disconnected_notify);
                                return;
                            }
                            if writer
                                .write_all(
                                    format!("HTTP/1.1 200 OK\r\nContent-Type: {media}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").as_bytes(),
                                )
                                .await
                                .is_err()
                            {
                                mark_disconnected(&disconnected, &disconnected_notify);
                                return;
                            }
                            if !gate_before_headers
                                && wait_gate_or_disconnect(&first_gate, &mut reader).await
                            {
                                mark_disconnected(&disconnected, &disconnected_notify);
                                return;
                            }
                            for chunk in &first_chunks {
                                if write_chunk(&mut writer, chunk).await.is_err() {
                                    mark_disconnected(&disconnected, &disconnected_notify);
                                    return;
                                }
                            }
                            first_sent.store(true, Ordering::SeqCst);
                            first_written.notify_waiters();
                            if wait_gate_or_disconnect(&final_gate, &mut reader).await {
                                mark_disconnected(&disconnected, &disconnected_notify);
                                return;
                            }
                            for chunk in &final_chunks {
                                if write_chunk(&mut writer, chunk).await.is_err() {
                                    mark_disconnected(&disconnected, &disconnected_notify);
                                    return;
                                }
                            }
                            final_sent.store(true, Ordering::SeqCst);
                            final_written.notify_waiters();
                            if wait_gate_or_disconnect(&eof_gate, &mut reader).await {
                                mark_disconnected(&disconnected, &disconnected_notify);
                                return;
                            }
                            if fail_body {
                                let _ = writer.write_all(b"10\r\ntruncated").await;
                            } else if writer.write_all(b"0\r\n\r\n").await.is_ok() {
                                body_eof.store(true, Ordering::SeqCst);
                            } else {
                                mark_disconnected(&disconnected, &disconnected_notify);
                            }
                            let _ = writer.shutdown().await;
                        });
                    }
                }
            }
        });
        Self {
            address,
            attempts,
            first_gate,
            final_gate,
            eof_gate,
            first_sent,
            first_written,
            final_sent,
            final_written,
            body_eof,
            disconnected,
            disconnected_notify,
            stop,
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    pub fn release_first(&self) {
        self.first_gate.open();
    }

    pub fn release_final(&self) {
        self.final_gate.open();
    }

    pub fn release_eof(&self) {
        self.eof_gate.open();
    }

    pub fn body_eof(&self) -> bool {
        self.body_eof.load(Ordering::SeqCst)
    }

    pub async fn wait_for_first(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = self.first_written.notified();
                if self.first_sent.load(Ordering::SeqCst) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("first upstream chunks within timeout");
    }

    pub async fn wait_for_attempts(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.attempts() < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("upstream attempts within timeout");
    }

    pub async fn wait_for_final(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = self.final_written.notified();
                if self.final_sent.load(Ordering::SeqCst) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("final upstream chunks within timeout");
    }

    pub async fn wait_for_disconnect(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = self.disconnected_notify.notified();
                if self.disconnected.load(Ordering::SeqCst) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("gateway closes upstream socket within timeout");
    }
}

impl Drop for GatedUpstreamFixture {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.first_gate.open();
        self.final_gate.open();
        self.eof_gate.open();
    }
}

async fn wait_gate_or_disconnect(gate: &Gate, reader: &mut tokio::net::tcp::OwnedReadHalf) -> bool {
    let mut byte = [0_u8; 1];
    tokio::select! {
        () = gate.wait() => false,
        result = reader.read(&mut byte) => matches!(result, Ok(0) | Err(_)),
    }
}

async fn write_chunk(writer: &mut tokio::net::tcp::OwnedWriteHalf, bytes: &[u8]) -> io::Result<()> {
    writer
        .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
        .await?;
    writer.write_all(bytes).await?;
    writer.write_all(b"\r\n").await
}

fn mark_disconnected(disconnected: &AtomicBool, notify: &Notify) {
    disconnected.store(true, Ordering::SeqCst);
    notify.notify_waiters();
}

pub async fn read_request(socket: &mut TcpStream) -> io::Result<CapturedRequest> {
    let mut received = Vec::new();
    let header_end = loop {
        if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if received.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture header too large",
            ));
        }
        let mut chunk = [0_u8; 4096];
        let count = socket.read(&mut chunk).await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request header EOF",
            ));
        }
        received.extend_from_slice(&chunk[..count]);
    };
    let header_text = std::str::from_utf8(&received[..header_end - 4])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 fixture header"))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let target = request_line
        .split_ascii_whitespace()
        .nth(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request target"))?
        .to_owned();
    let mut headers = Vec::new();
    let mut content_length = 0_usize;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "malformed fixture header")
        })?;
        let value = value.trim_start().as_bytes().to_vec();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = std::str::from_utf8(&value)
                .ok()
                .and_then(|number| number.parse().ok())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad content length"))?;
        }
        headers.push((name.to_owned(), value));
    }
    while received.len() < header_end + content_length {
        let mut chunk = [0_u8; 4096];
        let count = socket.read(&mut chunk).await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request body EOF",
            ));
        }
        received.extend_from_slice(&chunk[..count]);
    }
    Ok(CapturedRequest {
        target,
        headers,
        body: received[header_end..header_end + content_length].to_vec(),
    })
}

pub async fn send_raw(address: SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).await.expect("connect gateway");
    let marker = b"Host: localhost\r\n";
    let request = if let Some(index) = request
        .windows(marker.len())
        .position(|bytes| bytes == marker)
    {
        [
            request[..index].to_vec(),
            format!("Host: {address}\r\n").into_bytes(),
            request[index + marker.len()..].to_vec(),
        ]
        .concat()
    } else {
        request.to_vec()
    };
    socket.write_all(&request).await.expect("write request");
    socket.shutdown().await.expect("finish request write");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), socket.read_to_end(&mut response))
        .await
        .expect("gateway response timeout")
        .expect("read gateway response");
    response
}

pub async fn open_raw(address: SocketAddr, request: &[u8]) -> TcpStream {
    let mut socket = TcpStream::connect(address).await.expect("connect gateway");
    let marker = b"Host: localhost\r\n";
    let request = if let Some(index) = request
        .windows(marker.len())
        .position(|bytes| bytes == marker)
    {
        [
            request[..index].to_vec(),
            format!("Host: {address}\r\n").into_bytes(),
            request[index + marker.len()..].to_vec(),
        ]
        .concat()
    } else {
        request.to_vec()
    };
    socket.write_all(&request).await.expect("write request");
    socket
}

pub fn abort_socket(socket: TcpStream) {
    let socket = socket.into_std().expect("convert test socket");
    socket2::SockRef::from(&socket)
        .set_linger(Some(Duration::ZERO))
        .expect("configure test RST");
    drop(socket);
}

pub async fn read_until(socket: &mut TcpStream, needle: &[u8]) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut response = Vec::new();
        loop {
            if response
                .windows(needle.len())
                .any(|window| window == needle)
            {
                return response;
            }
            let mut chunk = [0_u8; 4096];
            let count = socket
                .read(&mut chunk)
                .await
                .expect("read gateway response");
            assert_ne!(count, 0, "gateway response ended before expected bytes");
            response.extend_from_slice(&chunk[..count]);
        }
    })
    .await
    .expect("expected gateway bytes within timeout")
}

pub fn status(response: &[u8]) -> u16 {
    let first_line = response
        .split(|byte| *byte == b'\n')
        .next()
        .expect("status line");
    let line = std::str::from_utf8(first_line).expect("UTF-8 status line");
    line.split_ascii_whitespace()
        .nth(1)
        .expect("status code")
        .parse()
        .expect("numeric status")
}

pub fn response_header<'a>(response: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    response[..header_end]
        .split(|byte| *byte == b'\n')
        .skip(1)
        .filter_map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let separator = line.iter().position(|byte| *byte == b':')?;
            let key = std::str::from_utf8(&line[..separator]).ok()?;
            key.eq_ignore_ascii_case(name).then_some(
                line[separator + 1..]
                    .strip_prefix(b" ")
                    .unwrap_or(&line[separator + 1..]),
            )
        })
        .next()
}

pub fn response_body(response: &[u8]) -> &[u8] {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("response header terminator");
    &response[header_end + 4..]
}
