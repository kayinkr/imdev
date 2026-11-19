use imdev::{core::{Request,ResponseData},store::Store};
#[test]
fn corrupt_file_is_preserved_and_failed_commit_does_not_mutate_memory(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("corrupt.json");std::fs::write(&path,"not json").unwrap();assert!(Store::open(path.clone()).is_err());assert_eq!(std::fs::read_to_string(path).unwrap(),"not json");
 let parent=dir.path().join("blocked");let mut store=Store::open(parent.join("state.json")).unwrap();std::fs::write(&parent,"unchanged").unwrap();let request=Request{url:"http://127.0.0.1/".into(),..Request::default()};assert!(store.save("Example",&request).is_err());assert!(store.saved.is_empty());assert_eq!(std::fs::read_to_string(parent).unwrap(),"unchanged");
}
#[test]
fn history_retains_only_100_entries_and_error_text_is_not_written(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let mut store=Store::open(path.clone()).unwrap();let request=Request{url:"http://127.0.0.1/private/SECRET?key=SECRET".into(),..Request::default()};for _ in 0..102{store.record(&request,&Err("SECRET".into())).unwrap();}assert_eq!(store.history.len(),100);assert!(store.history.iter().all(|h|h.error));assert!(!std::fs::read_to_string(&path).unwrap().contains("SECRET"));
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;assert_eq!(std::fs::metadata(path).unwrap().permissions().mode()&0o777,0o600);}
}
#[test]
fn saved_templates_and_history_are_redacted_transactional_and_survive_reload(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let mut store=Store::open(path.clone()).unwrap();
 let request=Request{url:"http://user:SECRET@example.com/private/SECRET?token=SECRET".into(),method:"POST".into(),query:vec![("q".into(),"SECRET".into())],headers:vec![("Authorization".into(),"SECRET".into())],body_type:"text".into(),body:"SECRET".into(),timeout_ms:1000};
 store.save("Example",&request).unwrap();store.record(&request,&Ok(ResponseData{status:200,status_text:"OK".into(),headers:vec![("X".into(),"SECRET".into())],body:"SECRET".into(),elapsed_ms:12,truncated:false})).unwrap();
 let raw=std::fs::read_to_string(&path).unwrap();assert!(!raw.contains("SECRET"));assert_eq!(store.saved[0].request.url,"http://example.com/");assert_eq!(store.history[0].url,"http://example.com");assert_eq!(store.history[0].elapsed_ms,12);
 let mut loaded=Store::open(path).unwrap();assert_eq!(loaded.saved.len(),1);assert_eq!(loaded.history.len(),1);let id=loaded.saved[0].id.clone();loaded.remove(&id).unwrap();loaded.clear_history().unwrap();assert!(loaded.saved.is_empty());assert!(loaded.history.is_empty());
}
