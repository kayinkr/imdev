use imdev::core::Request;
#[test]
fn validation_rejects_invalid_input_without_echoing_secrets(){
 let good=Request{url:"http://127.0.0.1/".into(),..Request::default()};assert!(good.validate().is_ok());
 for bad in [Request{url:"file:///SECRET".into(),..good.clone()},Request{url:"http://user:SECRET@localhost".into(),..good.clone()},Request{method:"TRACE".into(),..good.clone()},Request{timeout_ms:99,..good.clone()},Request{timeout_ms:120001,..good.clone()},Request{body_type:"json".into(),body:"SECRET".into(),method:"POST".into(),..good.clone()},Request{body_type:"text".into(),body:"SECRET".into(),..good.clone()},Request{headers:vec![("X-Test".into(),"SECRET\nSECRET".into())],..good.clone()}]{let error=bad.validate().unwrap_err();assert!(!error.contains("SECRET"));}
}
#[test]
fn pairs_preserve_duplicates_colons_and_empty_values(){
 let pairs=imdev::core::parse_pairs("Accept: application/json\nX-Link: https://example.com:443\ntag: one\ntag: two\nempty:\n").unwrap();
 assert_eq!(pairs,vec![("Accept".into(),"application/json".into()),("X-Link".into(),"https://example.com:443".into()),("tag".into(),"one".into()),("tag".into(),"two".into()),("empty".into(),"".into())]);
 assert!(imdev::core::parse_pairs("missing colon").is_err());assert!(imdev::core::parse_pairs(": empty key").is_err());
}
#[test]
fn safe_template_retains_structure_but_never_credentials(){
 let input=Request{url:"https://user:SECRET@example.com/private/SECRET?token=SECRET#SECRET".into(),method:"POST".into(),query:vec![("key".into(),"SECRET".into())],headers:vec![("Authorization".into(),"SECRET".into())],body_type:"json".into(),body:"SECRET".into(),timeout_ms:1000};
 let clean=input.sanitized().unwrap();assert_eq!(clean.url,"https://example.com/");assert_eq!(clean.query[0].1,"");assert_eq!(clean.headers[0].1,"");assert_eq!(clean.body,"");assert_eq!(clean.body_type,"none");assert!(!serde_json::to_string(&clean).unwrap().contains("SECRET"));assert_eq!(input.body,"SECRET");
}
