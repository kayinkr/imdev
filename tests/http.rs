use imdev::core::{Request,execute};
use tokio::{net::TcpListener,io::{AsyncReadExt,AsyncWriteExt},sync::watch};
use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
struct Mock{url:String,hits:Arc<AtomicUsize>,task:tokio::task::JoinHandle<()>}
impl Drop for Mock{fn drop(&mut self){self.task.abort();}}
async fn mock()->Mock{
 let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let hits=Arc::new(AtomicUsize::new(0));let count=hits.clone();
 let task=tokio::spawn(async move{loop{let Ok((mut stream,_))=listener.accept().await else{break};let count=count.clone();tokio::spawn(async move{
 let mut data=Vec::new();let mut block=[0u8;4096];let end;
 loop{let n=stream.read(&mut block).await.unwrap_or(0);if n==0{return;}data.extend_from_slice(&block[..n]);if let Some(i)=data.windows(4).position(|x|x==b"\r\n\r\n"){end=i+4;break;}}
 let headers=String::from_utf8_lossy(&data[..end]).to_string();let length=headers.lines().find_map(|s|s.to_lowercase().strip_prefix("content-length:").and_then(|s|s.trim().parse::<usize>().ok())).unwrap_or(0);
 while data.len()<end+length{let n=stream.read(&mut block).await.unwrap_or(0);if n==0{break;}data.extend_from_slice(&block[..n]);}
 count.fetch_add(1,Ordering::SeqCst);let first=headers.lines().next().unwrap_or("");
 if first.contains("/slow"){tokio::time::sleep(std::time::Duration::from_millis(800)).await;}
 let (status,extra,body)=if first.contains("/redirect"){("302 Found","Location: /must-not-run\r\n",String::new())}else if first.contains("/big"){("200 OK","","a".repeat(2*1024*1024+17))}else if first.contains("/head"){("200 OK","",String::new())}else{("418 I'm a teapot","",String::from_utf8_lossy(&data).to_string())};
 let header=format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nX-Mock: local-only\r\nX-Multi: one\r\nX-Multi: two\r\n{extra}Connection: close\r\n\r\n",body.len());let _=stream.write_all(header.as_bytes()).await;let _=stream.write_all(body.as_bytes()).await;
 });}});Mock{url,hits,task}
}
#[tokio::test]
async fn timeout_cancellation_and_pre_cancel_return_promptly_without_retries(){
 let server=mock().await;
 let req=Request{url:format!("{}/slow",server.url),timeout_ms:100,..Request::default()};
 let (_tx,rx)=watch::channel(false);let start=std::time::Instant::now();let error=execute(req.clone(),rx).await.unwrap_err();assert!(error.contains("초과"));assert!(start.elapsed()<std::time::Duration::from_millis(600));assert_eq!(server.hits.load(Ordering::SeqCst),1);
 let (tx,rx)=watch::channel(false);let task=tokio::spawn(execute(Request{timeout_ms:2000,..req.clone()},rx));
 tokio::time::timeout(std::time::Duration::from_secs(1),async{while server.hits.load(Ordering::SeqCst)<2{tokio::time::sleep(std::time::Duration::from_millis(2)).await;}}).await.unwrap();tx.send(true).unwrap();assert!(task.await.unwrap().unwrap_err().contains("취소"));assert_eq!(server.hits.load(Ordering::SeqCst),2);
 let (_tx,rx)=watch::channel(true);assert!(execute(req,rx).await.unwrap_err().contains("취소"));assert_eq!(server.hits.load(Ordering::SeqCst),2);
}
#[tokio::test]
async fn redirect_is_not_followed_and_body_is_bounded(){
 let server=mock().await;let (_tx,rx)=watch::channel(false);
 let redirect=execute(Request{url:format!("{}/redirect",server.url),..Request::default()},rx.clone()).await.unwrap();assert_eq!(redirect.status,302);assert!(redirect.headers.iter().any(|(k,v)|k=="location"&&v=="/must-not-run"));assert_eq!(server.hits.load(Ordering::SeqCst),1);
 let big=execute(Request{url:format!("{}/big",server.url),..Request::default()},rx).await.unwrap();assert!(big.truncated);assert_eq!(big.body.len(),2*1024*1024);assert_eq!(server.hits.load(Ordering::SeqCst),2);
}
#[tokio::test]
async fn all_seven_methods_and_plain_text_are_supported(){
 let server=mock().await;let (_tx,rx)=watch::channel(false);
 for method in ["GET","POST","PUT","PATCH","DELETE","HEAD","OPTIONS"]{
 let body=!["GET","HEAD"].contains(&method);
 let response=execute(Request{url:format!("{}/{}",server.url,if method=="HEAD"{"head"}else{"echo"}),method:method.into(),body_type:if body{"text"}else{"none"}.into(),body:if body{"plain text"}else{""}.into(),..Request::default()},rx.clone()).await.unwrap();assert_eq!(response.status,if method=="HEAD"{200}else{418});if body{assert!(response.body.ends_with("plain text"));assert!(response.body.to_lowercase().contains("content-type: text/plain;charset=utf-8"));}if method=="HEAD"{assert!(response.body.is_empty());}}
 assert_eq!(server.hits.load(Ordering::SeqCst),7);
}
#[tokio::test]
async fn compressed_local_response_is_decoded_like_baseline_fetch(){
 let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let task=tokio::spawn(async move{let(mut s,_)=listener.accept().await.unwrap();let mut request=[0;4096];let _=s.read(&mut request).await;let bytes=[31,139,8,0,0,0,0,0,2,255,75,206,207,45,40,74,45,46,78,77,81,200,201,79,78,204,81,0,114,10,242,243,138,83,1,192,161,224,26,25,0,0,0];s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).as_bytes()).await.unwrap();s.write_all(&bytes).await.unwrap();});
 let(_tx,rx)=watch::channel(false);let response=execute(Request{url,..Request::default()},rx).await.unwrap();assert_eq!(response.body,"compressed local response");task.await.unwrap();
}
#[tokio::test]
async fn real_local_post_preserves_method_query_headers_json_and_error_response(){
 let server=mock().await;let req=Request{url:format!("{}/echo?old=1",server.url),method:"POST".into(),query:vec![("q".into(),"hello world".into()),("q".into(),"two".into())],headers:vec![("X-Demo".into(),"example".into())],body_type:"json".into(),body:"{\"ok\":true}".into(),timeout_ms:2000};
 let (_tx,rx)=watch::channel(false);let res=execute(req,rx).await.unwrap();assert_eq!(res.status,418);assert_eq!(res.status_text,"I'm a teapot");assert!(res.body.starts_with("POST /echo?old=1&q=hello+world&q=two HTTP/1.1"));assert!(res.body.to_lowercase().contains("x-demo: example"));assert!(res.body.to_lowercase().contains("content-type: application/json"));assert!(res.body.ends_with("{\"ok\":true}"));assert_eq!(res.headers.iter().filter(|(k,_)|k=="x-multi").count(),2);assert_eq!(server.hits.load(Ordering::SeqCst),1);
}
