#[path = "../../../product/tests/support/fixture.rs"]
mod fixture;
use llmgw::config::{Accounting,Auth,CancelPolicy,Config,Endpoint,Limit,Model,Quota,Root,Upstream};
use llmgw::server::{self,RuntimeCredentials};
use llmgw::admission::{ManualClock,quota::{Ledger,Decision,RequestCost}};
use std::time::Duration;
fn known(n:u64)->Limit {Limit::Known(n.try_into().unwrap())}
fn config(address:std::net::SocketAddr)->Config {Config {
 listen:"127.0.0.1:0".parse().unwrap(),concurrency:2,cancel_policy:CancelPolicy::Drain,accounting:Accounting::Actual,
 upstream:Upstream{api_base:format!("http://{address}/v1").parse().unwrap(),auth:Auth::None},
 quota:Quota{rpm:Limit::Unlimited,tpm:known(1000)},
 models:vec![Model{id:"synthetic".into(),max_output_tokens:None}],
 roots:vec![Root{id:"review".into(),endpoints:vec![Endpoint::ChatCompletions,Endpoint::Responses,Endpoint::Messages],models:vec!["synthetic".into()]}]
}}
async fn debit(path:&str,sse:&str)->(u128,u128){
 let raw=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",sse.len(),sse);
 let upstream=fixture::UpstreamFixture::start(raw.into_bytes()).await;
 let clock=ManualClock::default();
 let gateway=server::testing::spawn_with_clock(config(upstream.address()),RuntimeCredentials::new(b"synthetic-data",b"synthetic-control",None).unwrap(),clock.clone(),None).await.unwrap();
 clock.advance_to(Duration::from_secs(60));
 let body=if path=="responses" {r#"{"model":"synthetic","input":"synthetic","max_output_tokens":300}"#}else{r#"{"model":"synthetic","messages":[],"max_tokens":300}"#};
 let req=format!("POST /r/review/v1/{path} HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: synthetic-data\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
 let response=fixture::send_raw(gateway.address(),req.as_bytes()).await;
 assert_eq!(fixture::status(&response),200);
 tokio::time::timeout(Duration::from_secs(2),async {while server::testing::quota_snapshot(&gateway).cleanups!=1 {tokio::task::yield_now().await;}}).await.unwrap();
 let debit=server::testing::quota_snapshot(&gateway).tpm_debited;
 assert_eq!(upstream.attempts(),1);
 gateway.shutdown().await.unwrap();
 (debit,body.len() as u128+300)
}
#[tokio::test]
async fn valid_chat_final_usage_settles(){let (d,_)=debit("chat/completions","data: {\"choices\":[],\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n").await;assert_eq!(d,23);}
#[tokio::test]
async fn chat_nonfinal_usage_must_not_refund(){let(d,e)=debit("chat/completions","data: {\"choices\":[{\"delta\":{\"content\":\"synthetic\"}}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n").await;assert_eq!(d,e,"nonempty choices is not the supported final usage report");}
#[tokio::test]
async fn chat_missing_choices_usage_must_not_refund(){let(d,e)=debit("chat/completions","data: {\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n").await;assert_eq!(d,e,"missing choices is not the supported final usage report");}
#[tokio::test]
async fn overflowing_usage_keeps_reservation(){let(d,e)=debit("chat/completions","data: {\"choices\":[],\"usage\":{\"prompt_tokens\":18446744073709551615,\"completion_tokens\":1}}\n\ndata: [DONE]\n\n").await;assert_eq!(d,e);}
#[tokio::test]
async fn responses_failed_after_completed_must_not_refund(){let(d,e)=debit("responses","data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\ndata: {\"type\":\"response.failed\"}\n\n").await;assert_eq!(d,e,"conflicting terminal states must remain unknown");}
#[test]
fn independent_ledger_interleaving(){
 let mut l=Ledger::new(Quota{rpm:known(2),tpm:known(100)},Accounting::Actual,2,Duration::ZERO);
 let admit=|l:&mut Ledger,t,c| match l.admit(Duration::from_secs(t),RequestCost::exact_fixture(c)){Decision::Admitted(id)=>id,d=>panic!("{d:?}")};
 let a=admit(&mut l,60,80);l.start(Duration::from_secs(60),a);
 let b=admit(&mut l,61,20);l.start(Duration::from_secs(61),b);
 l.finish(Duration::from_secs(119),b,Some(1));
 let c=admit(&mut l,120,90);l.start(Duration::from_secs(120),c);
 l.finish(Duration::from_secs(121),a,Some(30));
 assert_eq!(l.snapshot(Duration::from_secs(121)).tpm_debited,120);
 assert!(!l.finish(Duration::from_secs(122),a,Some(0)));
 l.finish(Duration::from_secs(122),c,None);
 assert_eq!(l.snapshot(Duration::from_secs(180)).tpm_debited,30);
 assert_eq!(l.snapshot(Duration::from_secs(181)).tpm_debited,0);
}
