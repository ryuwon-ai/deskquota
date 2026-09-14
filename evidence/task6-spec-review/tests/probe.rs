use llmgw::admission::{Admission, AcquireError, Hold, ManualClock};
use llmgw::admission::quota::RequestCost;
use llmgw::config::{Config, Root, Endpoint, Quota, Limit, Accounting, CancelPolicy, Auth, Upstream};
use std::{future::Future, pin::Pin, task::{Context, Poll}, time::Duration};
type Ticket<'a> = Pin<Box<dyn Future<Output=Result<Hold,AcquireError>> + 'a>>;
fn cfg(cap:u8, roots:usize)->Config {Config{listen:"127.0.0.1:0".parse().unwrap(),concurrency:cap,cancel_policy:CancelPolicy::Drain,accounting:Accounting::Reserved,upstream:Upstream{api_base:"http://127.0.0.1:9/v1".parse().unwrap(),auth:Auth::None},quota:Quota{rpm:Limit::Unlimited,tpm:Limit::Known(100.try_into().unwrap())},models:vec![],roots:(0..roots).map(|i|Root{id:format!("r{i}"),endpoints:vec![Endpoint::Models],models:vec![]}).collect()}}
fn enqueue(a:&Admission,r:usize,c:u64,wait:u64)->Ticket<'_>{Box::pin(a.acquire(r,RequestCost::exact_fixture(c),Endpoint::Models,Duration::from_secs(wait)))}
fn poll(t:&mut Ticket<'_>)->Poll<Result<Hold,AcquireError>>{let w=futures_util::task::noop_waker();t.as_mut().poll(&mut Context::from_waker(&w))}
fn pending(t:&mut Ticket<'_>){assert!(poll(t).is_pending());}
fn ready(t:&mut Ticket<'_>)->Hold {match poll(t){Poll::Ready(Ok(h))=>h,_=>panic!("expected admission")}}
fn setup(cap:u8,roots:usize)->(Admission,ManualClock){let c=ManualClock::default();let a=Admission::new(&cfg(cap,roots),Some(c.clone()));c.advance_to(Duration::from_secs(60));(a,c)}
#[tokio::test]
async fn nonhead_expiration_reclaims_full_queue_without_head_progress(){
 let(a,c)=setup(1,2);let mut seed=enqueue(&a,0,1,120);let seed=ready(&mut seed);
 let mut head=enqueue(&a,0,80,120);pending(&mut head);
 let mut expired=enqueue(&a,0,1,2);pending(&mut expired);
 let mut rest:Vec<_>=(0..62).map(|_|enqueue(&a,1,1,120)).collect();for t in &mut rest {pending(t)}
 let mut full=enqueue(&a,1,1,120);assert!(matches!(poll(&mut full),Poll::Ready(Err(AcquireError::QueueFull))));
 c.advance_to(Duration::from_secs(62));let mut refill=enqueue(&a,1,1,120);pending(&mut refill);
 assert!(matches!(poll(&mut expired),Poll::Ready(Err(AcquireError::Deadline))));pending(&mut head);
 drop(rest);drop(refill);drop(head);drop(seed);assert_eq!(a.snapshot().active,0);assert_eq!(a.snapshot().starts,0);assert_eq!(a.snapshot().cleanups,1);
}
#[tokio::test]
async fn admitted_unpolled_cancel_retains_no_quota_and_does_not_rewind_cursor(){
 let(a,c)=setup(1,3);let mut first=enqueue(&a,0,90,120);let first=ready(&mut first);
 let mut b=enqueue(&a,1,80,120);pending(&mut b);let mut c2=enqueue(&a,2,80,120);pending(&mut c2);drop(first);
 pending(&mut c2);assert_eq!(a.snapshot().tpm_held,80);assert_eq!(a.snapshot().starts,0);
 c.advance_to(Duration::from_secs(121));assert_eq!(a.snapshot().tpm_held,80);drop(b);
 let mut h=ready(&mut c2);assert_eq!(h.wait_started(),Duration::from_secs(60));assert_eq!(h.queue_deadline(),Duration::from_secs(180));h.start();drop(h);
 let s=a.snapshot();assert_eq!((s.active,s.starts,s.cleanups,s.tpm_debited,s.tpm_held),(0,1,3,80,0));
}
#[tokio::test]
async fn two_full_sixteen_root_rounds_do_not_create_false_bypass_barrier(){
 let(a,_)=setup(1,16);let mut seed=enqueue(&a,0,0,120);let seed=ready(&mut seed);
 let mut rounds:Vec<Vec<_>>=(0..2).map(|_|(0..16).map(|r|enqueue(&a,r,0,120)).collect()).collect();
 for round in &mut rounds {for t in round {pending(t)}}drop(seed);
 for round in &mut rounds {for r in (1..16).chain([0]) {let h=ready(&mut round[r]);drop(h)}}assert_eq!(a.snapshot().cleanups,33);
}
#[tokio::test]
async fn younger_blocked_trigger_protects_oldest_fit_and_survives_trigger_removal(){
 let(a,_)=setup(1,11);let mut seed=enqueue(&a,1,50,120);let mut seed=ready(&mut seed);seed.start();
 let mut oldest=enqueue(&a,0,1,120);pending(&mut oldest);let mut trigger=enqueue(&a,1,80,120);pending(&mut trigger);
 let mut rr:Vec<_>=(2..10).map(|r|enqueue(&a,r,1,120)).collect();for t in &mut rr {pending(t)}drop(seed);
 for t in &mut rr[..7] {drop(ready(t));}
 let eighth=ready(&mut rr[7]);drop(trigger);let mut late=enqueue(&a,10,1,120);pending(&mut late);drop(eighth);
 pending(&mut late);let oldest=ready(&mut oldest);drop(oldest);drop(ready(&mut late));assert_eq!(a.snapshot().active,0);
}
#[tokio::test(start_paused = true)]
async fn production_clock_age_timer_and_quota_expiry_preserve_fifo_original_age(){
 let a=Admission::new(&cfg(2,2),None);
 let mut seed=enqueue(&a,1,50,120);pending(&mut seed);tokio::time::advance(Duration::from_secs(60)).await;let mut seed=ready(&mut seed);seed.start();drop(seed);
 let mut heavy=enqueue(&a,0,80,120);pending(&mut heavy);let mut second=enqueue(&a,0,70,120);pending(&mut second);
 tokio::time::advance(Duration::from_millis(4999)).await;let mut small=enqueue(&a,1,1,120);drop(ready(&mut small));
 tokio::time::advance(Duration::from_millis(1)).await;let mut blocked=enqueue(&a,1,1,120);pending(&mut blocked);
 drop(heavy);pending(&mut blocked);tokio::time::advance(Duration::from_secs(55)).await;
 let mut second=ready(&mut second);assert_eq!(second.wait_started(),Duration::from_secs(60));second.start();drop(second);drop(ready(&mut blocked));assert_eq!(a.snapshot().starts,2);
}
#[tokio::test]
async fn invalid_roots_and_impossible_cost_cannot_create_a_queue_share(){
 let(a,_)=setup(1,16);let mut bad=enqueue(&a,16,0,120);assert!(matches!(poll(&mut bad),Poll::Ready(Err(AcquireError::InvalidRoot))));
 let mut impossible=enqueue(&a,0,101,120);assert!(matches!(poll(&mut impossible),Poll::Ready(Err(AcquireError::EstimateExceedsBudget))));
 for id in ["".to_owned(),"r/1".to_owned(),"가".to_owned(),"x".repeat(65)] {let mut c=cfg(1,1);c.roots[0].id=id;assert!(llmgw::server::validate_start_config(&c).is_err());}
 assert!(llmgw::server::validate_start_config(&cfg(1,17)).is_err());let mut dup=cfg(1,2);dup.roots[1].id="r0".into();assert!(llmgw::server::validate_start_config(&dup).is_err());
 assert_eq!(a.snapshot().active,0);
}
#[tokio::test(start_paused = true)]
async fn idle_waiter_is_woken_by_production_age_timer_without_external_notification(){
 use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
 struct Counter(AtomicUsize);impl std::task::Wake for Counter{fn wake(self:Arc<Self>){self.0.fetch_add(1,Ordering::SeqCst);}fn wake_by_ref(self:&Arc<Self>){self.0.fetch_add(1,Ordering::SeqCst);}}
 let a=Admission::new(&cfg(2,2),None);tokio::time::advance(Duration::from_secs(60)).await;
 let mut seed=enqueue(&a,1,50,120);let mut seed=ready(&mut seed);seed.start();drop(seed);
 let counter=Arc::new(Counter(AtomicUsize::new(0)));let w=std::task::Waker::from(counter.clone());
 let mut head=enqueue(&a,0,80,120);assert!(head.as_mut().poll(&mut Context::from_waker(&w)).is_pending());
 counter.0.store(0,Ordering::SeqCst);tokio::time::advance(Duration::from_millis(4900)).await;assert_eq!(counter.0.load(Ordering::SeqCst),0);
 tokio::time::advance(Duration::from_millis(110)).await;tokio::task::yield_now().await;assert!(counter.0.load(Ordering::SeqCst)>0,"age timer must wake blocked ticket");
 assert!(head.as_mut().poll(&mut Context::from_waker(&w)).is_pending());counter.0.store(0,Ordering::SeqCst);
 tokio::time::advance(Duration::from_secs(10)).await;tokio::task::yield_now().await;assert_eq!(counter.0.load(Ordering::SeqCst),0,"already-aged barrier cannot spin on a past age deadline");
}
async fn raw(addr:std::net::SocketAddr,request:Vec<u8>)->Vec<u8>{
 use tokio::io::{AsyncReadExt,AsyncWriteExt};
 tokio::time::timeout(Duration::from_secs(3),async {let mut s=tokio::net::TcpStream::connect(addr).await.unwrap();s.write_all(&request).await.unwrap();let mut out=vec![];s.read_to_end(&mut out).await.unwrap();out}).await.unwrap()
}
fn control_request(extra:&str)->Vec<u8>{format!("GET /_llmgw/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{extra}\r\n").into_bytes()}
async fn status_value(addr:std::net::SocketAddr)->serde_json::Value{let b=raw(addr,control_request("X-LLMGW-Control-Token: review-control\r\n")).await;assert!(b.starts_with(b"HTTP/1.1 200"));let split=b.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;serde_json::from_slice(&b[split..]).unwrap()}
async fn until_status(addr:std::net::SocketAddr,key:&str,n:u64)->serde_json::Value{tokio::time::timeout(Duration::from_secs(3),async{loop{let s=status_value(addr).await;if s["admission"][key]==n{return s}tokio::task::yield_now().await;}}).await.unwrap()}
#[tokio::test]
async fn owned_wire_metadata_shares_provisional_rpm_and_safe_authenticated_status(){
 use tokio::io::{AsyncReadExt,AsyncWriteExt};use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();let attempts=Arc::new(AtomicUsize::new(0));let count=attempts.clone();
 let fixture=tokio::spawn(async move{loop{let(mut s,_)=listener.accept().await.unwrap();let mut buf=vec![];loop{let mut b=[0;1024];let n=s.read(&mut b).await.unwrap();if n==0{break}buf.extend_from_slice(&b[..n]);if buf.windows(4).any(|w|w==b"\r\n\r\n"){break}}
  count.fetch_add(1,Ordering::SeqCst);s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();}});
 let mut c=cfg(2,2);c.upstream.api_base=format!("http://{addr}/v1").parse().unwrap();c.quota.rpm=Limit::Known(1.try_into().unwrap());
 let clock=ManualClock::default();let(gate,rx)=tokio::sync::watch::channel(false);
 let g=llmgw::server::testing::spawn_with_clock(c,llmgw::server::RuntimeCredentials::new(b"review-data",b"review-control",None).unwrap(),clock.clone(),Some(rx)).await.unwrap();let ga=g.address();
 assert!(raw(ga,control_request("X-LLMGW-Token: review-data\r\n")).await.starts_with(b"HTTP/1.1 401"));
 assert!(raw(ga,control_request("X-LLMGW-Control-Token: review-control\r\nOrigin: http://localhost\r\n")).await.starts_with(b"HTTP/1.1 403"));
 clock.advance_to(Duration::from_secs(60));
 let wire=|r|format!("GET /r/r{r}/v1/models HTTP/1.1\r\nHost: localhost\r\nX-LLMGW-Token: review-data\r\nX-Pi-Session-Id: invented-child\r\nConnection: close\r\n\r\n").into_bytes();
 let first=tokio::spawn(raw(ga,wire(0)));until_status(ga,"active",1).await;
 let second=tokio::spawn(raw(ga,wire(1)));let s=until_status(ga,"queue_length",1).await;
 assert_eq!(s["admission"]["active"],1);assert_eq!(s["admission"]["rpm_debited"],"0");assert_eq!(s["admission"]["tpm_held"],"0");assert_eq!(s["admission"]["roots"].as_array().unwrap().len(),2);assert_eq!(s["admission"]["roots"][1]["queue_length"],1);assert_eq!(s["admission"]["blocked_reason"],"rpm");assert_eq!(attempts.load(Ordering::SeqCst),0);
 gate.send(true).unwrap();assert!(first.await.unwrap().starts_with(b"HTTP/1.1 200"));let s=until_status(ga,"active",0).await;assert_eq!(s["admission"]["rpm_debited"],"1");assert_eq!(s["admission"]["tpm_debited"],"0");assert_eq!(attempts.load(Ordering::SeqCst),1);
 clock.advance_to(Duration::from_secs(120));assert!(second.await.unwrap().starts_with(b"HTTP/1.1 200"));until_status(ga,"active",0).await;assert_eq!(attempts.load(Ordering::SeqCst),2);assert_eq!(llmgw::server::testing::quota_snapshot(&g).starts,2);
 g.shutdown().await.unwrap();fixture.abort();let _=fixture.await;
}
