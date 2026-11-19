use imdev::core::Request;
use imdev::policy::*;
#[test]
fn auth_is_scoped_late_bound_and_conflicts_are_errors() {
 let mut d = draft("https://example.com/api");
 let mut s = AuthSession { origin: "https://EXAMPLE.com:443/".into(), token: "synthetic-old".into() };
 assert_eq!(prepare(&d, &s).unwrap().headers, vec![("Authorization".into(), "Bearer synthetic-old".into())]);
 s.token = "synthetic-new".into();
 assert_eq!(prepare(&d, &s).unwrap().headers[0].1, "Bearer synthetic-new");
 assert!(d.request.headers.is_empty());
 for u in ["http://example.com/api", "https://other.example/api", "https://example.com:444/api"] { d.request.url=u.into(); assert!(prepare(&d,&s).unwrap().headers.is_empty()); d.auth=AuthMode::Common; assert!(prepare(&d,&s).is_err()); d.auth=AuthMode::Auto; }
 d.request.url="https://example.com/api".into();
 d.request.headers.push(("authorization".into(), "manual".into()));
 assert!(prepare(&d,&s).is_err());
 d.auth=AuthMode::None; assert!(prepare(&d,&s).is_ok());
 d.request.headers.push(("Authorization".into(), "second".into()));
 assert!(prepare(&d,&s).is_err());
 d.request.headers.clear(); d.auth=AuthMode::Common; s.token.clear(); assert!(prepare(&d,&s).is_err());
 d.auth=AuthMode::Auto; assert!(prepare(&d,&s).is_err());
 d.auth=AuthMode::None; assert!(prepare(&d,&s).is_ok());
}
fn draft(url: &str) -> RequestDraft { RequestDraft { request: Request { url: url.into(), ..Default::default() }, ..Default::default() } }
#[test]
fn snapshot_preserves_general_request_exactly() {
 let mut d=draft("http://localhost:80/a/nested?x=1&x=&encoded=%2f%20#local");
 d.request.query=vec![("x".into(),"2".into()),("blank".into(),"".into())];
 d.request.method="POST".into(); d.request.body_type="json".into(); d.request.body="{ \n \"ordinary\": [1, 2] }".into();
 d.request.headers=vec![("X-Ordinary".into(),"kept".into())]; d.request.timeout_ms=4321;
 let saved=snapshot(&d,&AuthSession::default()).unwrap();
 let mut expected=d.clone(); expected.request.url=expected.request.url.split('#').next().unwrap().into(); expected.auth=AuthMode::None;
 assert_eq!(saved.draft,expected); assert!(saved.missing.iter().any(|s|s.contains("fragment")));
 let serialized=serde_json::to_string(&saved).unwrap(); let restored:Snapshot=serde_json::from_str(&serialized).unwrap(); assert_eq!(restored.draft, expected);
}
#[test]
fn snapshot_scrubs_builtin_and_user_secrets_without_losing_structure() {
 let mut d=draft("http://localhost:8080/v1/custom%2Fsecret/items?x=1&x=&accessToken=url-synthetic&private=custom-query");
 d.request.method="POST".into(); d.request.body_type="json".into();
 d.request.body=r#"{ "nested": [{"clientSecret":"json-synthetic","okay":"keep","private":"custom-json"}], "copy":"session-synthetic" }"#.into();
 d.request.headers=vec![("Authorization".into(),"Bearer header-synthetic".into()),("Cookie".into(),"cookie-synthetic".into()),("X-Private".into(),"custom-header".into()),("X-Okay".into(),"keep".into())];
 d.request.query=vec![("api_key".into(),"editor-synthetic".into()),("ordinary".into(),"keep".into())];
 d.exclusions=Exclusions{query_keys:vec!["private".into()],header_names:vec!["x-private".into()],json_pointers:vec!["/nested/0/private".into()],path_segments:vec![2],body:false};
 let s=AuthSession{origin:"http://localhost:8080".into(),token:"session-synthetic".into()};
 let saved=snapshot(&d,&s).unwrap(); let serialized=serde_json::to_string(&saved).unwrap();
 for secret in ["custom%2Fsecret","url-synthetic","custom-query","json-synthetic","custom-json","session-synthetic","header-synthetic","cookie-synthetic","custom-header","editor-synthetic"] { assert!(!serialized.contains(secret),"secret persisted"); }
 assert_eq!(saved.draft.auth,AuthMode::Common);
 assert!(saved.draft.request.url.contains("x=1&x=&accessToken="));
 assert_eq!(saved.draft.request.headers[3].1,"keep");
 assert!(saved.missing.len()>=9); assert!(prepare(&saved.draft,&s).is_err());
 assert!(!format!("{saved:?}").contains("keep"));
}
#[test]
fn markers_and_copied_tokens_are_blocked_in_every_mode() {
 let s=AuthSession{origin:"https://safe.example".into(),token:"synthetic-copied-token".into()};
 for mode in [AuthMode::Auto,AuthMode::None,AuthMode::Common] {
  for value in ["synthetic-copied-token","%73ynthetic-copied-token",REDACTED,"%5f%5fIMDEV_REENTER_SECRET__"] {
   let mut d=draft(&format!("https://other.example/path?unknown={value}")); d.auth=mode.clone();
   let error=prepare(&d,&s).unwrap_err(); assert!(!error.contains("synthetic-copied-token")); assert!(!error.contains("other.example"));
  }
 }
 let mut d=draft("https://other.example/path"); d.auth=AuthMode::None; d.request.method="POST".into();d.request.body_type="json".into();
 d.request.body=r#"{"renamed":"\u0073ynthetic-copied-token"}"#.into();
 assert!(prepare(&d,&s).is_err());
 let saved=snapshot(&d,&s).unwrap(); assert!(!serde_json::to_string(&saved).unwrap().contains("copied-token"));
}
#[test]
fn form_and_full_body_exclusions_keep_type_and_block_send() {
 let mut d=draft("http://localhost/token"); d.auth=AuthMode::None;d.request.method="POST".into();d.request.body_type="text".into();
 d.request.body="client_secret=form-synthetic&grant_type=client_credentials&x=&x=1".into();
 let saved=snapshot(&d,&AuthSession::default()).unwrap(); assert_eq!(saved.draft.request.body,format!("client_secret={REDACTED}&grant_type=client_credentials&x=&x=1"));
 assert_eq!(saved.draft.auth,AuthMode::None); assert!(prepare(&saved.draft,&AuthSession::default()).is_err());
 for (kind,body) in [("text","unknown-synthetic"),("json",r#"{"unknown":"unknown-synthetic"}"#)] {
  d.request.body_type=kind.into();d.request.body=body.into();d.exclusions.body=true;
  let saved=snapshot(&d,&AuthSession::default()).unwrap();assert_eq!(saved.draft.request.body_type,kind);assert!(!serde_json::to_string(&saved).unwrap().contains("unknown-synthetic"));assert!(prepare(&saved.draft,&AuthSession::default()).is_err());
  if kind=="json" { assert!(serde_json::from_str::<serde_json::Value>(&saved.draft.request.body).is_ok()); }
 }
}
#[test]
fn url_userinfo_is_rejected_and_path_indices_are_one_based_nonempty() {
 for u in ["http://user:pass@localhost/a","http://@localhost/a"] { assert!(snapshot(&draft(u),&AuthSession::default()).is_err()); }
 let mut d=draft("http://localhost//first///second%2Fembedded/third");d.exclusions.path_segments=vec![2];
 let saved=snapshot(&d,&AuthSession::default()).unwrap();assert_eq!(saved.draft.request.url,format!("http://localhost//first///{REDACTED}/third"));
 for u in ["https://@example.com/","https://example.com/a/..","https://example.com/."] { assert!(canonical_origin(u).is_err()); }
}
#[test]
fn encoded_form_values_and_numeric_json_copies_do_not_leak_session_tokens() {
 for (token,body,kind) in [("synthetic token", "unknown=synthetic+token", "text"),("123456789",r#"{"ordinary":123456789}"#,"json")] {
  let session=AuthSession{origin:"http://localhost:1111".into(),token:token.into()};
  let mut d=draft("http://localhost:2222/path");d.auth=AuthMode::None;d.request.method="POST".into();d.request.body_type=kind.into();d.request.body=body.into();
  assert!(prepare(&d,&session).is_err());
  let saved=snapshot(&d,&session).unwrap();assert!(!serde_json::to_string(&saved).unwrap().contains(token));assert!(saved.draft.request.body.contains(REDACTED));
 }
}
#[test]
fn session_token_spanning_url_syntax_is_not_persisted() {
 let session=AuthSession{origin:"https://safe.example".into(),token:"synthetic-left=synthetic-right".into()};
 let d=draft("https://other.example/path?synthetic-left=synthetic-right");
 let saved=snapshot(&d,&session).unwrap();assert!(!serde_json::to_string(&saved).unwrap().contains(&session.token));assert!(prepare(&saved.draft,&session).is_err());
}
#[test]
fn canonical_origins_are_strict_and_normalized() {
 assert_eq!(canonical_origin("HTTPS://Example.COM:443/"), Ok("https://example.com".into()));
 for u in ["https://example.com/a", "https://example.com/?x", "https://example.com/#x", "https://u:p@example.com", "file:///x"] { assert!(canonical_origin(u).is_err()); }
}
