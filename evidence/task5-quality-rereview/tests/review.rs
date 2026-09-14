#[path = "../../../product/tests/support/fixture.rs"]
mod fixture;
use llmgw::admission::{ManualClock, quota::{Ledger, Decision, RequestCost}};
use llmgw::config::{Accounting, Auth, CancelPolicy, Config, Endpoint, Limit, Model, Quota, Root, Upstream};
use llmgw::server::{self, RuntimeCredentials};
use std::time::Duration;
fn known(n:u64)->Limit { Limit::Known(n.try_into().unwrap()) }
fn config(address:std::net::SocketAddr)->Config { Config {
 listen:"127.0.0.1:0".parse().unwrap(), concurrency:16, cancel_policy:CancelPolicy::Drain, accounting:Accounting::Actual,
 upstream:Upstream{api_base:format!("http://{address}/v1").parse().unwrap(),auth:Auth::None},
 quota:Quota{rpm:known(100),tpm:known(10000)},
 models:vec![Model{id:"synthetic".into(),max_output_tokens:None}],
 roots:vec![Root{id:"review".into(),endpoints:vec![Endpoint::ChatCompletions,Endpoint::Messages,Endpoint::Models],models:vec!["synthetic".into()]}]
}}
fn request(path:&str,body:&str)->Vec<u8>{format!("{} /r/review/v1/{path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",if path=="models"{"GET"}else{"POST"},body.len()).into_bytes()}
async fn settled(g:&server::GatewayHandle,n:u64){tokio::time::timeout(Duration::from_secs(3),async {while server::testing::quota_snapshot(g).cleanups<n {tokio::task::yield_now().await;}}).await.unwrap();}
async fn messages_debit(sse:&str)->(u128,u128){
 let raw=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len());
 let up=fixture::UpstreamFixture::start(raw.into_bytes()).await;
 let clock=ManualClock::default();
 let g=server::testing::spawn_with_clock(config(up.address()),RuntimeCredentials::new(b"synthetic-data",b"synthetic-control",None).unwrap(),clock.clone(),None).await.unwrap();
 clock.advance_to(Duration::from_secs(60));
 let body=r#"{"model":"synthetic","messages":[],"max_tokens":300}"#;
 let response=fixture::send_raw(g.address(),&request("messages",body)).await;
 settled(&g,1).await;
 let q=server::testing::quota_snapshot(&g);
 assert_eq!(fixture::status(&response),200);assert_eq!(fixture::response_body(&response),sse.as_bytes());
 assert_eq!((q.active,q.starts,q.cleanups,up.attempts()),(0,1,1,1));
 g.shutdown().await.unwrap();
 (q.tpm_debited,body.len() as u128+300)
}
const START:&str="event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n";
const DELTA:&str="event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n";
const STOP:&str="event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
#[tokio::test]
async fn messages_normal_order_is_positive_control(){let(d,_)=messages_debit(&format!("{START}{DELTA}{STOP}")).await;assert_eq!(d,2);}
#[tokio::test]
async fn messages_output_usage_after_stop_must_remain_unknown(){let(d,e)=messages_debit(&format!("{START}{STOP}{DELTA}")).await;assert_eq!(d,e,"final delta must precede message_stop");}
#[tokio::test]
async fn messages_stop_before_all_usage_must_remain_unknown(){let(d,e)=messages_debit(&format!("{STOP}{START}{DELTA}")).await;assert_eq!(d,e,"terminal marker cannot qualify future usage");}

#[test]
fn saturation_preserves_late_debt_and_active_identity(){
 use llmgw::admission::quota::MAX_ENTRIES;
 let t=Duration::from_secs;
 let mut l=Ledger::new(Quota{rpm:known(u64::MAX),tpm:known(u64::MAX)},Accounting::Actual,16,t(0));
 let add=|l:&mut Ledger,at,c|match l.admit(t(at),RequestCost::exact_fixture(c)){Decision::Admitted(id)=>id,x=>panic!("{x:?}")};
 let old=add(&mut l,60,1);assert!(l.start(t(60),old));
 for _ in 1..MAX_ENTRIES {let id=add(&mut l,60,1);assert!(l.start(t(60),id));assert!(l.finish(t(60),id,None));}
 assert!(matches!(l.admit(t(119),RequestCost::Metadata),Decision::Wait(Some(_))));
 assert_eq!(l.snapshot(t(120)).retained,1);
 let fresh=add(&mut l,120,1);assert!(l.start(t(120),fresh));
 assert!(l.finish(t(120),old,Some(u64::MAX)));assert_eq!(l.snapshot(t(120)).tpm_debited,u128::from(u64::MAX)+1);
 assert!(!l.finish(t(121),old,Some(0)));assert!(!l.cancel(t(121),old));
 assert!(l.finish(t(121),fresh,None));assert!(matches!(l.admit(t(179),RequestCost::exact_fixture(1)),Decision::Wait(Some(_))));
 assert_eq!(l.snapshot(t(180)).retained,0);assert_eq!(l.snapshot(t(180)).cleanups,MAX_ENTRIES as u64+1);
}

#[tokio::test(flavor="multi_thread",worker_threads=4)]
async fn twenty_contenders_prestart_expiry_cancellation_and_release(){
 use tokio::io::AsyncReadExt;
 let up=fixture::UpstreamFixture::start(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec()).await;
 let clock=ManualClock::default();let(gate,rx)=tokio::sync::watch::channel(false);
 let g=server::testing::spawn_with_clock(config(up.address()),RuntimeCredentials::new(b"synthetic-data",b"synthetic-control",None).unwrap(),clock.clone(),Some(rx)).await.unwrap();
 clock.advance_to(Duration::from_secs(60));
 let body=r#"{"model":"synthetic","messages":[],"max_tokens":300}"#;
 let req=request("messages",body);let mut sockets=Vec::new();
 for i in 0..20 {
  sockets.push(fixture::open_raw(g.address(),&req).await);
  if i<16 {tokio::time::timeout(Duration::from_secs(3),async {while server::testing::quota_snapshot(&g).active<i+1 {tokio::task::yield_now().await;}}).await.unwrap();}
 }
 clock.advance_to(Duration::from_secs(500));
 let q=server::testing::quota_snapshot(&g);
 assert_eq!((q.active,q.starts,q.tpm_debited),(16,0,0));assert_eq!(q.tpm_held,16*(body.len() as u128+300));assert_eq!(up.attempts(),0);
 for socket in sockets.drain(0..8){fixture::abort_socket(socket);}
 settled(&g,8).await;
 gate.send(true).unwrap();
 for mut socket in sockets {let mut data=vec![];tokio::time::timeout(Duration::from_secs(3),socket.read_to_end(&mut data)).await.unwrap().unwrap();assert_eq!(fixture::status(&data),200);}
 settled(&g,20).await;
 let q=server::testing::quota_snapshot(&g);
 assert_eq!((q.active,q.cleanups,q.starts,q.rpm_debited,q.tpm_held,up.attempts()),(0,20,12,12,0,12));
 assert_eq!(q.tpm_debited,12*(body.len() as u128+300));
 g.shutdown().await.unwrap();
}

#[tokio::test]
async fn qualified_usage_cannot_be_updated_after_stop_through_sse_event_name(){
 for late in [
  "event: message_start\ndata: {\"message\":{\"usage\":{\"input_tokens\":2}}}\n\n",
  "event: message_delta\ndata: {\"usage\":{\"output_tokens\":2}}\n\n",
 ] {
  let(d,e)=messages_debit(&format!("{START}{DELTA}{STOP}{late}")).await;
  assert_eq!(d,e,"named usage-bearing events after stop must invalidate earlier qualified usage");
 }
}

#[tokio::test]
async fn ordinary_post_stop_events_do_not_invalidate_qualified_usage(){
 let tail="event: message_start\ndata: {\"message\":{}}\n\nevent: message_delta\ndata: {\"delta\":{\"stop_reason\":\"end_turn\"}}\n\nevent: synthetic.notice\ndata: {\"message\":\"ordinary error text\"}\n\n";
 let(d,_)=messages_debit(&format!("{START}{DELTA}{STOP}{tail}{STOP}")).await;
 assert_eq!(d,2);
}
