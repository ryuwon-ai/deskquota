
#[path = "../../../product/tests/support/fixture.rs"] mod fixture;
use llmgw::config::{Accounting,Auth,CancelPolicy,Config,Endpoint,Limit,Model,Quota,Root,Upstream};
use llmgw::server::{self,RuntimeCredentials};
use std::time::Duration;
use tokio::io::{AsyncReadExt,AsyncWriteExt};
fn config(a: std::net::SocketAddr) -> Config { Config {
listen:"127.0.0.1:0".parse().unwrap(), concurrency:1,cancel_policy:CancelPolicy::Drain,accounting:Accounting::Reserved,retry_transient_429:true,
upstream:Upstream{api_base:format!("http://{a}/v1").parse().unwrap(),auth:Auth::None},quota:Quota{rpm:Limit::Unlimited,tpm:Limit::Unknown},
models:vec![Model{id:"fixture".into(),max_output_tokens:None}],roots:vec![Root{id:"a".into(),models:vec!["fixture".into()],endpoints:vec![Endpoint::Models]}]}}
async fn raw(a:std::net::SocketAddr)->Vec<u8>{let mut s=tokio::net::TcpStream::connect(a).await.unwrap();s.write_all(b"GET /r/a/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-quality-data\r\nConnection: close\r\n\r\n").await.unwrap();let mut b=vec![]; let result=tokio::time::timeout(Duration::from_secs(3),s.read_to_end(&mut b)).await;println!("raw_read={result:?} response_len={}",b.len());b}
#[tokio::test]
async fn truncated_429_before_downstream_headers_has_an_http_outcome(){
let body=br#"{"error":{"code":"slow_down"}}"#;
let wire=[b"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n".to_vec(),body.to_vec()].concat();
let up=fixture::UpstreamFixture::start(wire).await;
let gw=server::spawn(config(up.address()),RuntimeCredentials::new(b"synthetic-quality-data",b"synthetic-quality-control",None).unwrap()).await.unwrap();
let response=raw(gw.address()).await;
let attempts=up.attempts();let snapshot=server::testing::quota_snapshot(&gw);
println!("OUTCOME {}",serde_json::json!({"case":"truncated429","ingress":1,"upstream_http_attempts":attempts,"response_bytes":response.len(),"http_head":response.starts_with(b"HTTP/1.1 "),"active":snapshot.active,"starts":snapshot.starts}));
gw.shutdown().await.unwrap();assert_eq!(attempts,1);assert_eq!(snapshot.active,0);assert!(response.starts_with(b"HTTP/1.1 "),"pre-header probe failure must return an HTTP error outcome rather than an empty socket");
}

#[tokio::test]
async fn complete_unknown_429_is_preserved_positive_control(){
let body=br#"{"error":{"code":"unclassified"}}"#;
let wire=[format!("HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes(),body.to_vec()].concat();
let up=fixture::UpstreamFixture::start(wire).await;
let gw=server::spawn(config(up.address()),RuntimeCredentials::new(b"synthetic-quality-data",b"synthetic-quality-control",None).unwrap()).await.unwrap();
let response=raw(gw.address()).await;let attempts=up.attempts();let snapshot=server::testing::quota_snapshot(&gw);
println!("OUTCOME {}",serde_json::json!({"case":"complete429-control","ingress":1,"upstream_http_attempts":attempts,"response_bytes":response.len(),"http_head":response.starts_with(b"HTTP/1.1 "),"active":snapshot.active,"starts":snapshot.starts}));
gw.shutdown().await.unwrap();assert_eq!(attempts,1);assert_eq!(snapshot.active,0);assert_eq!(fixture::status(&response),429);assert_eq!(fixture::response_body(&response),body);
}
#[tokio::test]
async fn gated_truncated_429_cannot_flush_a_known_error_as_normal_response(){
let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
let (sent,receive)=tokio::sync::oneshot::channel();let (release,released)=tokio::sync::oneshot::channel();
let worker=tokio::spawn(async move {let (mut socket,_)=listener.accept().await.unwrap();fixture::read_until(&mut socket,b"\r\n\r\n").await;
socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n{\"error\":{\"code\":\"slow_down\"}}").await.unwrap();sent.send(()).unwrap();released.await.unwrap();socket.shutdown().await.unwrap();});
let gw=server::spawn(config(address),RuntimeCredentials::new(b"synthetic-quality-data",b"synthetic-quality-control",None).unwrap()).await.unwrap();
let mut s=tokio::net::TcpStream::connect(gw.address()).await.unwrap();s.write_all(b"GET /r/a/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-quality-data\r\nConnection: close\r\n\r\n").await.unwrap();receive.await.unwrap();
let mut response=vec![];let before=tokio::time::timeout(Duration::from_millis(100),s.read_buf(&mut response)).await;assert!(before.is_err());assert!(response.is_empty());
release.send(()).unwrap();let read=tokio::time::timeout(Duration::from_secs(2),s.read_to_end(&mut response)).await;worker.await.unwrap();let snapshot=server::testing::quota_snapshot(&gw);
println!("OUTCOME {}",serde_json::json!({"case":"gated-truncated429","ingress":1,"upstream_http_attempts":1,"response_bytes":response.len(),"http_head":response.starts_with(b"HTTP/1.1 "),"active":snapshot.active,"starts":snapshot.starts,"pre_error_no_downstream_bytes":true,"read_completed":matches!(read,Ok(Ok(_)))}));
gw.shutdown().await.unwrap();assert_eq!(snapshot.active,0);assert_eq!(snapshot.starts,1);assert!(response.starts_with(b"HTTP/1.1 "),"known pre-header probe error returned no HTTP response");
}
