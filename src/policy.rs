//! Replay policy. Session credentials never belong to a saved draft.
use serde::{Deserialize, Serialize};
use crate::core::Request;
use url::Url;

pub const REDACTED: &str = "__IMDEV_REENTER_SECRET__";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AuthMode { #[default] Auto, None, Common }
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Exclusions {
 pub query_keys: Vec<String>, pub header_names: Vec<String>, pub json_pointers: Vec<String>,
 pub path_segments: Vec<usize>, pub body: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestDraft { pub request: Request, pub auth: AuthMode, pub exclusions: Exclusions }
impl Default for RequestDraft { fn default() -> Self { Self { request: Request::default(), auth: AuthMode::Auto, exclusions: Exclusions::default() } } }
impl std::fmt::Debug for RequestDraft {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("RequestDraft").field("request", &"[redacted]").field("auth", &self.auth).field("exclusions", &"[redacted]").finish() }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Snapshot { pub draft: RequestDraft, pub missing: Vec<String> }
#[derive(Default)]
pub struct AuthSession { pub origin: String, pub token: String }
fn decoded(input: &str) -> String {
 let bytes = input.as_bytes(); let mut out = Vec::new(); let mut i=0;
 while i<bytes.len() { if bytes[i]==b'%' && i+2<bytes.len() { if let (Some(a),Some(b))=((bytes[i+1] as char).to_digit(16),(bytes[i+2] as char).to_digit(16)) { out.push((a*16+b) as u8); i+=3; continue; } } out.push(bytes[i]); i+=1; }
 String::from_utf8_lossy(&out).into_owned()
}
fn contains(input: &str, needle: &str) -> bool {
 if needle.is_empty() { return false; }
 let mut value=input.to_owned();
 loop { if value.contains(needle) || value.replace('+'," ").contains(needle) { return true; } let next=decoded(&value); if next==value { break; } value=next; }
 false
}
fn json_contains(value:&serde_json::Value, needle:&str)->bool {
 match value { serde_json::Value::String(s)=>contains(s,needle), serde_json::Value::Object(map)=>map.iter().any(|(k,v)|contains(k,needle)||json_contains(v,needle)), serde_json::Value::Array(items)=>items.iter().any(|v|json_contains(v,needle)), _=>contains(&value.to_string(),needle) }
}
fn request_contains(r: &Request, needle: &str) -> bool {
 [&r.url,&r.method,&r.body_type,&r.body].into_iter().any(|s|contains(s,needle)) || r.query.iter().chain(r.headers.iter()).any(|(k,v)|contains(k,needle)||contains(v,needle)) || serde_json::from_str::<serde_json::Value>(&r.body).is_ok_and(|v|json_contains(&v,needle))
}
fn raw_parts(input:&str)->Result<(&str,&str),String> {
 let start=input.find("://").ok_or("An explicit HTTP(S) URL is required.")?+3;
 let end=input[start..].find(['/', '?', '#']).map(|i|start+i).unwrap_or(input.len());
 let authority=&input[start..end];
 if authority.contains('@') || authority.contains('\\') { return Err("URL credentials are not allowed.".into()); }
 Ok((authority,&input[end..]))
}
fn scope(draft: &RequestDraft, session: &AuthSession) -> Result<bool,String> {
 let u=Url::parse(&draft.request.url).map_err(|_|"Invalid request URL.".to_string())?;
 if session.origin.is_empty() { return Ok(false); }
 Ok(u.origin().ascii_serialization()==canonical_origin(&session.origin)?)
}
pub fn prepare(draft: &RequestDraft, session: &AuthSession) -> Result<Request,String> {
 if request_contains(&draft.request, REDACTED) { return Err("Re-enter excluded values before sending.".into()); }
 draft.request.validate()?;
 raw_parts(&draft.request.url)?;
 let count=draft.request.headers.iter().filter(|(k,_)|k.eq_ignore_ascii_case("authorization")).count();
 if count>1 { return Err("Duplicate Authorization headers are not allowed.".into()); }
 let same=scope(draft,session)?;
 if !same && request_contains(&draft.request,&session.token) { return Err("Session credential cannot be sent to another origin.".into()); }
 let inject=match draft.auth { AuthMode::None=>false, AuthMode::Auto=>same, AuthMode::Common=>{ if !same { return Err("Common auth requires its configured origin.".into()); } true } };
 let mut request=draft.request.clone();
 if inject {
  if session.token.is_empty() { return Err("Common auth requires a session token.".into()); }
  if count!=0 { return Err("Manual Authorization conflicts with common auth.".into()); }
  reqwest::header::HeaderValue::from_str(&format!("Bearer {}", session.token)).map_err(|_|"Invalid session token.".to_string())?;
  request.headers.push(("Authorization".into(),format!("Bearer {}",session.token)));
 }
 Ok(request)
}
fn sensitive(key: &str) -> bool {
 let normalized:String=key.chars().filter(|c|c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect();
 matches!(normalized.as_str(),"authorization"|"proxyauthorization"|"cookie"|"setcookie"|"apikey"|"xapikey"|"token"|"accesstoken"|"refreshtoken"|"idtoken"|"clientsecret"|"password"|"passwd"|"secret"|"authtoken"|"xauthtoken"|"xaccesstoken"|"sessiontoken"|"sessionid"|"csrftoken"|"xcsrftoken"|"xsrftoken"|"xxsrftoken")
}
fn listed(key:&str, list:&[String])->bool { list.iter().any(|k|k.eq_ignore_ascii_case(key)) }
fn mark(value:&mut String, location:String, missing:&mut Vec<String>) { *value=REDACTED.into(); missing.push(location); }
fn scrub_pairs(raw:&str, exclusions:&[String], token:&str, label:&str, missing:&mut Vec<String>)->String {
 raw.split('&').enumerate().map(|(i,pair)|{
  let (key,value)=pair.split_once('=').unwrap_or((pair,""));
  let decoded_key=url::form_urlencoded::parse(format!("{key}=").as_bytes()).next().map(|(k,_)|k.into_owned()).unwrap_or_default();
  let hide_key=contains(key,token);
  if hide_key || sensitive(&decoded_key) || listed(&decoded_key,exclusions) || contains(value,token) {
   missing.push(format!("{label} value [{}]",i+1));
   format!("{}={REDACTED}",if hide_key { REDACTED } else { key })
  } else { pair.into() }
 }).collect::<Vec<_>>().join("&")
}
fn scrub_json(value:&mut serde_json::Value, token:&str, missing:&mut Vec<String>, location:&str) {
 match value {
  serde_json::Value::Object(map)=>{
   let keys:Vec<String>=map.keys().cloned().collect();
   for (i,key) in keys.iter().enumerate() {
    let label=format!("{location}/field[{}]",i+1);
    if sensitive(key) || contains(key,token) { map.insert(key.clone(),serde_json::Value::String(REDACTED.into())); missing.push(label); }
    else if let Some(child)=map.get_mut(key) { scrub_json(child,token,missing,&label); }
    if contains(key,token) { let value=map.remove(key).unwrap(); map.insert(REDACTED.into(),value); }
   }
  }
  serde_json::Value::Array(items)=>for (i,child) in items.iter_mut().enumerate() { scrub_json(child,token,missing,&format!("{location}/item[{}]",i+1)); },
  serde_json::Value::String(s) if contains(s,token)=>mark(s,location.into(),missing),
  serde_json::Value::Number(_) | serde_json::Value::Bool(_) | serde_json::Value::Null if contains(&value.to_string(),token)=>{ *value=serde_json::Value::String(REDACTED.into()); missing.push(location.into()); },
  _=>{}
 }
}
pub fn snapshot(draft: &RequestDraft, session: &AuthSession) -> Result<Snapshot,String> {
 draft.request.validate()?;
 raw_parts(&draft.request.url)?;
 let mut saved=Snapshot { draft:draft.clone(), missing:Vec::new() };
 if saved.draft.auth==AuthMode::Auto { saved.draft.auth=if scope(draft,session)? { AuthMode::Common } else { AuthMode::None }; }
 if let Some(index)=saved.draft.request.url.find('#') { saved.draft.request.url.truncate(index); saved.missing.push("URL fragment removed (not sent over HTTP)".into()); }
 let token=&session.token;
 let exclusions=&draft.exclusions;
 let r=&mut saved.draft.request;
 let missing=&mut saved.missing;
 let (base,query)=r.url.split_once('?').map(|(a,b)|(a.to_owned(),Some(b.to_owned()))).unwrap_or((r.url.clone(),None));
 let start=base.find("://").ok_or("Invalid request URL.")?+3;
 let path_start=base[start..].find('/').map(|i|start+i).unwrap_or(base.len());
 let (authority,path)=base.split_at(path_start);
 let mut n=0;
 let path=path.split('/').map(|segment| { if segment.is_empty() { return String::new(); } n+=1; if exclusions.path_segments.contains(&n)||contains(segment,token) { missing.push(format!("URL path segment [{n}]")); REDACTED.to_owned() } else { segment.to_owned() } }).collect::<Vec<_>>().join("/");
 r.url=format!("{authority}{path}");
 if let Some(query)=query { r.url.push('?'); r.url.push_str(&scrub_pairs(&query,&exclusions.query_keys,token,"URL query",missing)); }
 if contains(authority,token) { mark(&mut r.url,"URL authority".into(),missing); }
 for (i,(key,value)) in r.query.iter_mut().enumerate() {
  if sensitive(key)||listed(key,&exclusions.query_keys)||contains(key,token)||contains(value,token) { mark(value,format!("Query editor value [{}]",i+1),missing); }
  if contains(key,token) { mark(key,format!("Query editor key [{}]",i+1),missing); }
 }
 for (i,(key,value)) in r.headers.iter_mut().enumerate() {
  if sensitive(key)||listed(key,&exclusions.header_names)||contains(key,token)||contains(value,token) { mark(value,format!("Header value [{}]",i+1),missing); }
  if contains(key,token) { mark(key,format!("Header name [{}]",i+1),missing); }
 }
 if exclusions.body {
  r.body=if r.body_type=="json" { serde_json::to_string(REDACTED).unwrap() } else { REDACTED.into() }; missing.push("Full body".into());
 } else if let Ok(mut json)=serde_json::from_str::<serde_json::Value>(&r.body) {
  let before=missing.len();
  for (i,pointer) in exclusions.json_pointers.iter().enumerate() { if let Some(value)=json.pointer_mut(pointer) { *value=serde_json::Value::String(REDACTED.into()); missing.push(format!("JSON pointer exclusion [{}]",i+1)); } }
  scrub_json(&mut json,token,missing,"JSON body");
  if missing.len()!=before { r.body=serde_json::to_string(&json).map_err(|_|"Cannot sanitize JSON body.".to_string())?; }
 } else {
  // Form field recognition is conservative; unknown free text needs an explicit exclusion.
  if r.body.contains('=') { r.body=scrub_pairs(&r.body,&[],token,"Form body",missing); }
  if contains(&r.body,token) { mark(&mut r.body,"Body containing session credential".into(),missing); }
 }
 // Catch copies spanning separators or JSON syntax, not only individual field values.
 if contains(&r.url,token) { mark(&mut r.url,"URL containing session credential".into(),missing); }
 if contains(&r.body,token) {
  r.body=if r.body_type=="json" { serde_json::to_string(REDACTED).unwrap() } else { REDACTED.into() };
  missing.push("Body containing session credential".into());
 }
 // Exclusion names are also persisted metadata; never retain a copied session credential.
 for (label,values) in [("query exclusions",&mut saved.draft.exclusions.query_keys),("header exclusions",&mut saved.draft.exclusions.header_names),("JSON exclusions",&mut saved.draft.exclusions.json_pointers)] { for value in values { if contains(value,token) { mark(value,label.into(),missing); } } }
 for (label,value) in [("Method",&mut r.method),("Body type",&mut r.body_type)] { if contains(value,token) { mark(value,label.into(),missing); } }
 Ok(saved)
}
pub fn canonical_origin(input: &str) -> Result<String, String> {
 let (_,tail)=raw_parts(input)?;
 if !matches!(tail,""|"/") { return Err("Common auth requires only an origin.".into()); }
 let u = Url::parse(input).map_err(|_| "Invalid common-auth origin.".to_string())?;
 if !matches!(u.scheme(), "http" | "https") || u.host_str().is_none() || !u.username().is_empty() || u.password().is_some() || !matches!(u.path(), "" | "/") || u.query().is_some() || u.fragment().is_some() { return Err("Common auth requires an HTTP(S) origin without credentials, path, query or fragment.".into()); }
 Ok(u.origin().ascii_serialization())
}
