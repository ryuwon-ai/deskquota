mod support;
use llmgw::config::LoadedConfig;
use llmgw::server::{self, RuntimeCredentials};
use support::fixture::{UpstreamFixture, response_body, send_raw, status};
#[tokio::test]
async fn declared_body_cap_uses_protocol_error_and_starts_no_wire_attempt() {
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
    )
    .await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let request = b"POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: 8388609\r\nConnection: close\r\n\r\n";
    let response = send_raw(gateway.address(), request).await;
    assert_eq!(status(&response), 413);
    let json: serde_json::Value = serde_json::from_slice(response_body(&response)).unwrap();
    assert_eq!(json["error"]["code"], "body_too_large");
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}

use std::time::Duration;
use support::fixture::open_raw;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn snapshot(address: std::net::SocketAddr) -> serde_json::Value {
    let raw = send_raw(address, b"GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\nConnection: close\r\n\r\n").await;
    serde_json::from_slice(response_body(&raw)).unwrap()
}
#[tokio::test]
async fn sixty_four_partial_bodies_release_memory_on_rejection_and_another_request_progresses() {
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
    )
    .await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        snapshot(gateway.address()).await["stored_request_bytes"],
        0,
        "observable held-byte budget is required for deterministic pressure gates"
    );
    let mut prefix = br#"{"model":"example-model","messages":[]}"#.to_vec();
    prefix.resize(512 * 1024, b' ');
    let header = format!(
        "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        prefix.len() + 1
    );
    let request = [header.as_bytes(), &prefix].concat();
    let mut clients = Vec::new();
    for _ in 0..64 {
        clients.push(open_raw(gateway.address(), &request).await);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while snapshot(gateway.address()).await["stored_request_bytes"] != 32 * 1024 * 1024 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all 64 partial bodies hold the full 32MiB and each needs an additional byte");
    assert_eq!(upstream.attempts(), 0);
    let mut victim = clients.pop().unwrap();
    victim.write_all(b" ").await.unwrap();
    let mut rejection = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), victim.read_to_end(&mut rejection))
        .await
        .expect("memory rejection never waits")
        .unwrap();
    assert_eq!(status(&rejection), 429);
    let error: serde_json::Value = serde_json::from_slice(response_body(&rejection)).unwrap();
    assert_eq!(error["error"]["code"], "gateway_memory_full");
    assert_eq!(
        snapshot(gateway.address()).await["stored_request_bytes"],
        63 * 512 * 1024
    );
    // All other partial senders remain incomplete; progress must use the released budget now.
    let small = br#"{"model":"example-model","messages":[]}"#;
    let request = [format!("POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", small.len()).into_bytes(), small.to_vec()].concat();
    let completed = tokio::time::timeout(
        Duration::from_secs(1),
        send_raw(gateway.address(), &request),
    )
    .await
    .expect("completable request immediately progresses after rejection");
    assert_eq!(status(&completed), 200);
    assert_eq!(upstream.attempts(), 1);
    for client in clients {
        support::fixture::abort_socket(client);
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while snapshot(gateway.address()).await["stored_request_bytes"] != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all partial body permits released after RST");
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn original_request_deadline_includes_a_slow_partial_body() {
    let upstream = UpstreamFixture::start(Vec::new()).await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    let gateway = server::testing::spawn_with_timeouts(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
        Duration::from_millis(80),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    let mut client = open_raw(gateway.address(), b"POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{").await;
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_millis(500), client.read_to_end(&mut raw))
        .await
        .expect("overall deadline starts before body collection")
        .unwrap();
    assert_eq!(status(&raw), 504);
    let error: serde_json::Value = serde_json::from_slice(response_body(&raw)).unwrap();
    assert_eq!(error["error"]["code"], "gateway_request_deadline");
    assert_eq!(snapshot(gateway.address()).await["stored_request_bytes"], 0);
    assert_eq!(upstream.attempts(), 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test]
async fn ingress_128_connections_cap_rejects_129th_and_reopens_after_release() {
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
    )
    .await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let health = b"GET /_llmgw/health HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\n\r\n";
    let mut clients = Vec::new();
    for _ in 0..128 {
        let mut client = open_raw(gateway.address(), health).await;
        assert_eq!(
            status(&support::fixture::read_until(&mut client, b"\r\n\r\n").await),
            200
        );
        clients.push(client);
    }
    let mut excess = tokio::net::TcpStream::connect(gateway.address())
        .await
        .unwrap();
    let mut byte = [0];
    let closed = tokio::time::timeout(Duration::from_secs(1), excess.read(&mut byte))
        .await
        .expect("129th ingress is not retained");
    assert!(matches!(closed, Ok(0) | Err(_)));
    assert_eq!(upstream.attempts(), 0);
    let mut released = clients.pop().unwrap();
    released.write_all(b"GET /_llmgw/health HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Control-Token: synthetic-control\r\nConnection: close\r\n\r\n").await.unwrap();
    released.read_to_end(&mut Vec::new()).await.unwrap();
    tokio::task::yield_now().await;
    let response = send_raw(gateway.address(), b"GET /r/pi-work/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nConnection: close\r\n\r\n").await;
    assert_eq!(status(&response), 200);
    assert_eq!(upstream.attempts(), 1);
    drop(clients);
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn slow_downstream_lifetimes_bound_delivery_and_release_workers_queue_and_bodies() {
    let raw = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 8388608\r\nConnection: close\r\n\r\n".to_vec(),
        vec![b'x'; 8 * 1024 * 1024],
    ]
    .concat();
    let upstream = UpstreamFixture::start(raw).await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    config.concurrency = 16;
    config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let mut clients = Vec::new();
    for _ in 0..24 {
        clients.push(open_raw(gateway.address(), b"GET /r/pi-work/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nConnection: close\r\n\r\n").await);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let s = snapshot(gateway.address()).await;
            assert!(s["queued_response_bytes"].as_u64().unwrap() <= 24 * 64 * 1024);
            assert!(s["admission"]["active"].as_u64().unwrap() <= 16);
            if s["admission"]["queue_length"] == 8
                && s["admission"]["active"] == 16
                && s["queued_response_bytes"].as_u64().unwrap() > 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("16 slow active readers and 8 waiting requests are all included");
    assert_eq!(upstream.attempts(), 16);
    let observed = snapshot(gateway.address()).await;
    println!(
        "slow_reader_connected=24 active={} queued={} completed_eof_before_cancel={} current_delivery_bytes={} max_delivery_bytes={}",
        observed["admission"]["active"],
        observed["admission"]["queue_length"],
        observed["response_body_eof"],
        observed["queued_response_bytes"],
        observed["max_queued_response_bytes"]
    );
    for client in clients {
        support::fixture::abort_socket(client);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let s = snapshot(gateway.address()).await;
            if s["active"] == 0
                && s["admission"]["queue_length"] == 0
                && s["queued_response_bytes"] == 0
                && s["stored_request_bytes"] == 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("slow reader disconnect drains active attempts and cancels queued ingress");
    // Queue cancellation races with drain completion, so actual starts may be 16..24, never hidden.
    assert!((16..=24).contains(&upstream.attempts()));
    println!(
        "slow_reader_ingress=24 actual_wire_attempts={} completed_client_responses=0 canceled_ingress=24",
        upstream.attempts()
    );
    gateway.shutdown().await.unwrap();
}
#[tokio::test]
async fn production_header_and_body_read_timeouts_release_slow_senders() {
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let mut header = open_raw(gateway.address(), b"GET /").await;
    let mut body = open_raw(gateway.address(), b"POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{").await;
    let mut head_result = Vec::new();
    let start = tokio::time::Instant::now();
    tokio::time::timeout(
        Duration::from_secs(12),
        header.read_to_end(&mut head_result),
    )
    .await
    .expect("10 second header read bound")
    .unwrap();
    assert!(start.elapsed() >= Duration::from_secs(9));
    let mut body_result = Vec::new();
    tokio::time::timeout(Duration::from_secs(22), body.read_to_end(&mut body_result))
        .await
        .expect("30 second body read bound")
        .unwrap();
    assert!(start.elapsed() >= Duration::from_secs(29));
    assert_eq!(status(&body_result), 408);
    let error: serde_json::Value = serde_json::from_slice(response_body(&body_result)).unwrap();
    assert_eq!(error["error"]["code"], "body_read_timeout");
    assert_eq!(snapshot(gateway.address()).await["stored_request_bytes"], 0);
    assert_eq!(server::testing::quota_snapshot(&gateway).starts, 0);
    gateway.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sixty_four_barrier_released_writers_account_for_every_memory_pressure_outcome() {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Barrier;
    use tokio::task::JoinSet;

    const WRITERS: usize = 64;
    const PARTIAL_BYTES: usize = 512 * 1024;
    const TOTAL_BYTES: usize = WRITERS * PARTIAL_BYTES;
    let upstream = UpstreamFixture::start(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
    )
    .await;
    let mut config = LoadedConfig::load("examples/fixture.toml").unwrap().config;
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.quota.rpm = llmgw::config::Limit::Unlimited;
    config.upstream.api_base = format!("http://{}/v1", upstream.address()).parse().unwrap();
    let gateway = server::spawn(
        config,
        RuntimeCredentials::new(b"synthetic-control", None).unwrap(),
    )
    .await
    .unwrap();
    let mut prefix = br#"{"model":"example-model","messages":[]}"#.to_vec();
    prefix.resize(PARTIAL_BYTES, b' ');
    let mut clients = Vec::new();
    for id in 0..WRITERS {
        let header = format!(
            "POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nX-Test-Contender: {id}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            PARTIAL_BYTES + 1
        );
        clients.push(open_raw(gateway.address(), &[header.as_bytes(), &prefix].concat()).await);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let held = snapshot(gateway.address()).await["stored_request_bytes"]
                .as_u64()
                .unwrap() as usize;
            assert!(held <= TOTAL_BYTES);
            if held == TOTAL_BYTES {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all64 partial bodies must be consumed and own32MiB before releasing writers");
    assert_eq!(upstream.attempts(), 0);

    // Every independently scheduled writer must arrive before this barrier opens.
    // Being connected or merely missing more network bytes is not the release gate.
    let barrier = Arc::new(Barrier::new(WRITERS + 1));
    let arrived = Arc::new(AtomicUsize::new(0));
    let write_attempts = Arc::new(AtomicUsize::new(0));
    let write_successes = Arc::new(AtomicUsize::new(0));
    let mut writers = JoinSet::new();
    for (id, mut socket) in clients.into_iter().enumerate() {
        let barrier = barrier.clone();
        let arrived = arrived.clone();
        let write_attempts = write_attempts.clone();
        let write_successes = write_successes.clone();
        writers.spawn(async move {
            arrived.fetch_add(1, Ordering::SeqCst);
            barrier.wait().await;
            let mut response = Vec::new();
            let terminal = tokio::time::timeout(Duration::from_secs(10), async {
                write_attempts.fetch_add(1, Ordering::SeqCst);
                socket.write_all(b" ").await?;
                write_successes.fetch_add(1, Ordering::SeqCst);
                socket.read_to_end(&mut response).await
            })
            .await;
            // Explicitly close/cancel every socket, including timeout/error outcomes.
            drop(socket);
            (id, terminal, response)
        });
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while arrived.load(Ordering::SeqCst) != WRITERS {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("64 independent writer tasks parked at the barrier");
    assert_eq!(write_attempts.load(Ordering::SeqCst), 0);
    assert_eq!(
        snapshot(gateway.address()).await["stored_request_bytes"],
        TOTAL_BYTES
    );
    tokio::time::timeout(Duration::from_secs(3), barrier.wait())
        .await
        .expect("all65 barrier participants release together");

    let mut completed_ids = BTreeSet::new();
    let mut rejected_ids = BTreeSet::new();
    let mut terminal_ids = BTreeSet::new();
    let mut canceled = 0;
    let mut unexpected = 0;
    let mut fresh_completed = false;
    let mut ownership_samples = 1;
    let mut max_observed_owned = TOTAL_BYTES;
    while let Some(joined) = writers.join_next().await {
        let (id, terminal, response) =
            joined.expect("writer task must report its terminal outcome");
        assert!(terminal_ids.insert(id));
        match terminal {
            Ok(Ok(_)) => match status(&response) {
                200 => {
                    assert_eq!(response_body(&response), b"{}");
                    completed_ids.insert(id);
                }
                429 => {
                    let error: serde_json::Value =
                        serde_json::from_slice(response_body(&response)).unwrap();
                    assert_eq!(error["error"]["code"], "gateway_memory_full");
                    rejected_ids.insert(id);
                }
                _ => unexpected += 1,
            },
            Ok(Err(_)) | Err(_) => canceled += 1,
        }
        let held = snapshot(gateway.address()).await["stored_request_bytes"]
            .as_u64()
            .unwrap() as usize;
        ownership_samples += 1;
        max_observed_owned = max_observed_owned.max(held);
        assert!(held <= TOTAL_BYTES);
        // Every observed memory429 must have dropped its partial body already.
        // Other contenders can reuse that memory, each retaining at most512KiB+1.
        assert!(held <= (WRITERS - rejected_ids.len()) * (PARTIAL_BYTES + 1));
        if !rejected_ids.is_empty() && !fresh_completed {
            let small = br#"{"model":"example-model","messages":[]}"#;
            let request = [format!("POST /r/pi-work/v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-LLMGW-Token: synthetic-data\r\nX-Test-Contender: fresh\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", small.len()).into_bytes(), small.to_vec()].concat();
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                send_raw(gateway.address(), &request),
            )
            .await
            .expect("fresh generation progresses after a concurrent memory rejection");
            assert_eq!(status(&response), 200);
            fresh_completed = true;
        }
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let state = snapshot(gateway.address()).await;
            if state["stored_request_bytes"] == 0
                && state["admission"]["active"] == 0
                && state["admission"]["queue_length"] == 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all body permits, active holds and queue tickets released");
    println!(
        "barrier_memory data_ingress={} writer_tasks={} barrier_arrivals={} write_attempts={} write_successes={} completed={} memory_rejected={} canceled={} unexpected={} pending={} fresh_completed={} upstream_attempts={} ownership_samples={} max_observed_owned={} final_owned=0",
        WRITERS + usize::from(fresh_completed),
        WRITERS,
        arrived.load(Ordering::SeqCst),
        write_attempts.load(Ordering::SeqCst),
        write_successes.load(Ordering::SeqCst),
        completed_ids.len(),
        rejected_ids.len(),
        canceled,
        unexpected,
        writers.len(),
        usize::from(fresh_completed),
        upstream.attempts(),
        ownership_samples,
        max_observed_owned,
    );
    assert_eq!(terminal_ids.len(), WRITERS);
    assert_eq!(write_attempts.load(Ordering::SeqCst), WRITERS);
    assert_eq!(write_successes.load(Ordering::SeqCst), WRITERS);
    assert_eq!(canceled, 0);
    assert_eq!(unexpected, 0);
    assert!(!rejected_ids.is_empty());
    assert_eq!(completed_ids.len() + rejected_ids.len(), WRITERS);
    assert!(fresh_completed);
    assert_eq!(upstream.attempts(), completed_ids.len() + 1);
    let captures = upstream.captures(completed_ids.len() + 1).await;
    let mut captured_ids = BTreeSet::new();
    let mut fresh_attempts = 0;
    for capture in captures {
        let id = capture.header("x-test-contender").unwrap();
        if id == b"fresh" {
            fresh_attempts += 1;
        } else {
            let id: usize = std::str::from_utf8(id).unwrap().parse().unwrap();
            assert_eq!(capture.body.len(), PARTIAL_BYTES + 1);
            assert!(
                captured_ids.insert(id),
                "no hidden repeated upstream attempt"
            );
            assert!(!rejected_ids.contains(&id));
        }
    }
    assert_eq!(captured_ids, completed_ids);
    assert_eq!(fresh_attempts, 1);
    gateway.shutdown().await.unwrap();
}
