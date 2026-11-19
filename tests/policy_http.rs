use imdev::core::{self, Request};
use imdev::policy::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server() -> (String, tokio::task::JoinHandle<String>) {
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
 let origin=format!("http://{}",listener.local_addr().unwrap());
 let handle=tokio::spawn(async move {
  tokio::time::timeout(std::time::Duration::from_secs(5),async move {
   let (mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();let mut buf=[0u8;2048];
   loop {
    let n=socket.read(&mut buf).await.unwrap(); assert_ne!(n,0);bytes.extend_from_slice(&buf[..n]);
    if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
     let headers=String::from_utf8_lossy(&bytes[..end]);
     let length=headers.lines().find_map(|line|{let(k,v)=line.split_once(':')?;k.eq_ignore_ascii_case("content-length").then(||v.trim().parse::<usize>().unwrap())}).unwrap_or(0);
     if bytes.len()>=end+4+length { break; }
    }
   }
   socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK").await.unwrap();
   String::from_utf8(bytes).unwrap()
  }).await.expect("loopback request timed out")
 });
 (origin,handle)
}
async fn send(draft:&RequestDraft,session:&AuthSession) {
 let (_tx,rx)=tokio::sync::watch::channel(false);
 let response=core::execute(prepare(draft,session).unwrap(),rx).await.unwrap();assert_eq!(response.status,200);assert_eq!(response.body,"OK");
}
#[tokio::test]
async fn replay_uses_current_session_token_only_at_exact_origin() {
 let (origin,received)=server().await;let (other,other_received)=server().await;
 let draft=RequestDraft{request:Request{url:format!("{origin}/v1/items?x=1&x="),query:vec![("x".into(),"2".into())],..Default::default()},..Default::default()};
 let mut session=AuthSession{origin:origin.clone(),token:"synthetic-previous".into()};
 let saved=snapshot(&draft,&session).unwrap();let encoded=serde_json::to_string(&saved).unwrap();assert!(!encoded.contains("synthetic-previous"));assert_eq!(saved.draft.auth,AuthMode::Common);
 session.token="synthetic-current".into();send(&saved.draft,&session).await;
 let wire=received.await.unwrap();assert!(wire.starts_with("GET /v1/items?x=1&x=&x=2 HTTP/1.1\r\n"));assert!(wire.to_ascii_lowercase().contains("authorization: bearer synthetic-current\r\n"));assert!(!wire.contains("synthetic-previous"));
 let mut foreign=draft.clone();foreign.request.url=format!("{other}/foreign");send(&foreign,&session).await;
 let wire=other_received.await.unwrap();assert!(!wire.to_ascii_lowercase().contains("authorization:"));assert!(!wire.contains("synthetic-current"));assert!(draft.request.headers.is_empty());
}
#[tokio::test]
async fn ordinary_request_round_trip_preserves_actual_body_headers_and_query_order() {
 let (origin,received)=server().await;
 let body="{ \n \"ordinary\": [1, 2] }";
 let draft=RequestDraft{request:Request{url:format!("{origin}/nested/path?x=1&x=&encoded=%2f%20"),method:"POST".into(),query:vec![("x".into(),"editor".into()),("blank".into(),"".into())],headers:vec![("X-Ordinary".into(),"kept".into())],body_type:"json".into(),body:body.into(),timeout_ms:4321},auth:AuthMode::None,exclusions:Exclusions::default()};
 let saved=snapshot(&draft,&AuthSession::default()).unwrap();let restored:Snapshot=serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();assert_eq!(restored.draft,draft);
 send(&restored.draft,&AuthSession::default()).await;let wire=received.await.unwrap();
 assert!(wire.starts_with("POST /nested/path?x=1&x=&encoded=%2f%20&x=editor&blank= HTTP/1.1\r\n"));assert!(wire.to_ascii_lowercase().contains("x-ordinary: kept\r\n"));assert!(wire.ends_with(body));assert!(!wire.to_ascii_lowercase().contains("authorization:"));
}
