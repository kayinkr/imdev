use serde::{Deserialize,Serialize};
use url::Url;
pub fn parse_pairs(text:&str)->Result<Vec<(String,String)>,String>{
 text.lines().enumerate().filter(|(_,line)|!line.trim().is_empty()).map(|(i,line)|{let (key,value)=line.split_once(':').ok_or_else(||format!("{}번째 줄: key: value 형식이 필요합니다.",i+1))?;if key.trim().is_empty(){return Err(format!("{}번째 줄: 키가 비어 있습니다.",i+1));}Ok((key.trim().into(),value.trim().into()))}).collect()
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct Request{
 pub url:String,pub method:String,pub query:Vec<(String,String)>,pub headers:Vec<(String,String)>,pub body_type:String,pub body:String,pub timeout_ms:u64,
}
impl Default for Request{fn default()->Self{Self{url:String::new(),method:"GET".into(),query:vec![],headers:vec![],body_type:"none".into(),body:String::new(),timeout_ms:30000}}}
#[derive(Clone,Debug)]
pub struct ResponseData{pub status:u16,pub status_text:String,pub headers:Vec<(String,String)>,pub body:String,pub elapsed_ms:u64,pub truncated:bool}

pub async fn execute(request:Request,mut cancel:tokio::sync::watch::Receiver<bool>)->Result<ResponseData,String>{
 request.validate()?;
 if *cancel.borrow(){return Err("요청이 취소되었습니다.".into());}
 let started=std::time::Instant::now();
 let operation=async{
 let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).retry(reqwest::retry::never()).no_proxy().timeout(std::time::Duration::from_millis(request.timeout_ms)).build().map_err(|_|"HTTP 클라이언트 초기화에 실패했습니다.".to_string())?;
 let mut url=Url::parse(&request.url).map_err(|_|"유효한 HTTP(S) URL이 필요합니다.".to_string())?;
 if !request.query.is_empty(){let mut query=url.query_pairs_mut();for (k,v) in &request.query{query.append_pair(k,v);}}
 let method=reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_|"잘못된 HTTP 메서드입니다.".to_string())?;
 let mut headers=reqwest::header::HeaderMap::new();for(k,v)in &request.headers{headers.append(reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|_|"잘못된 헤더 이름입니다.".to_string())?,reqwest::header::HeaderValue::from_str(v).map_err(|_|"잘못된 헤더 값입니다.".to_string())?);}
 if !headers.contains_key(reqwest::header::CONTENT_TYPE){
 if request.body_type=="json"{headers.insert(reqwest::header::CONTENT_TYPE,reqwest::header::HeaderValue::from_static("application/json"));}
 if request.body_type=="text"{headers.insert(reqwest::header::CONTENT_TYPE,reqwest::header::HeaderValue::from_static("text/plain;charset=UTF-8"));}
 }
 let mut builder=client.request(method,url).headers(headers);
 if request.body_type!="none"{builder=builder.body(request.body.clone());}
 let safe_error=|e:reqwest::Error|if e.is_timeout(){"요청 시간이 초과되었습니다.".to_string()}else{"요청 실패: 연결 또는 TLS 설정을 확인하세요.".to_string()};
 let mut res=builder.send().await.map_err(&safe_error)?;let status=res.status();let headers=res.headers().iter().map(|(k,v)|(k.as_str().into(),String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
 let mut bytes=Vec::new();let mut truncated=false;const LIMIT:usize=2*1024*1024;
 while let Some(chunk)=res.chunk().await.map_err(&safe_error)?{let remaining=LIMIT-bytes.len();if chunk.len()>remaining{bytes.extend_from_slice(&chunk[..remaining]);truncated=true;break;}bytes.extend_from_slice(&chunk);}
 Ok(ResponseData{status:status.as_u16(),status_text:status.canonical_reason().unwrap_or("").into(),headers,body:String::from_utf8_lossy(&bytes).into_owned(),elapsed_ms:started.elapsed().as_millis()as u64,truncated})
 };
 tokio::select!{biased;
 _=async{loop{if *cancel.borrow(){break;}if cancel.changed().await.is_err(){break;}}}=>Err("요청이 취소되었습니다.".into()),
 result=operation=>result,
 }
}

impl Request{
 pub fn validate(&self)->Result<(),String>{
 let url=Url::parse(&self.url).map_err(|_|"유효한 URL을 입력하세요.")?;
 if !["http","https"].contains(&url.scheme())||url.host_str().is_none(){return Err("HTTP(S) 주소만 허용합니다.".into());}
 if !url.username().is_empty()||url.password().is_some(){return Err("URL 인증정보는 허용하지 않습니다. 헤더 편집기를 사용하세요.".into());}
 if !["GET","POST","PUT","PATCH","DELETE","HEAD","OPTIONS"].contains(&self.method.as_str()){return Err("지원하지 않는 HTTP 메서드입니다.".into());}
 if !(100..=120000).contains(&self.timeout_ms){return Err("타임아웃은 100~120000ms 범위입니다.".into());}
 if !["none","json","text"].contains(&self.body_type.as_str()){return Err("지원하지 않는 본문 형식입니다.".into());}
 if ["GET","HEAD"].contains(&self.method.as_str())&&self.body_type!="none"{return Err("GET/HEAD 요청에는 본문을 사용할 수 없습니다.".into());}
 if self.body_type=="json"&&serde_json::from_str::<serde_json::Value>(&self.body).is_err(){return Err("유효한 JSON 본문을 입력하세요.".into());}
 if self.body.len()+self.url.len()+self.query.iter().chain(self.headers.iter()).map(|(k,v)|k.len()+v.len()).sum::<usize>()>1024*1024{return Err("요청 입력은 1MiB 이하로 제한됩니다.".into());}
 for (key,value) in &self.headers{reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|_|"올바른 헤더 이름을 입력하세요.")?;reqwest::header::HeaderValue::from_str(value).map_err(|_|"헤더 값에 허용되지 않는 문자가 있습니다.")?;}
 Ok(())
 }
 pub fn sanitized(&self)->Result<Self,String>{let url=Url::parse(&self.url).map_err(|_|"유효한 HTTP(S) URL을 입력하세요.".to_string())?;if !["http","https"].contains(&url.scheme())||url.host_str().is_none(){return Err("HTTP(S) URL만 허용합니다.".into());}
 Ok(Self{url:format!("{}/",url.origin().ascii_serialization()),method:self.method.clone(),query:self.query.iter().map(|(k,_)|(k.clone(),String::new())).collect(),headers:self.headers.iter().map(|(k,_)|(k.clone(),String::new())).collect(),body_type:"none".into(),body:String::new(),timeout_ms:self.timeout_ms})}
}
