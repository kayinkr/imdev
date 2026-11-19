#[path="support/crypto.rs"] mod crypto;
use imdev::{repository::Repository, policy::{self,RequestDraft,AuthSession},core::Request};
use std::fs;
fn plain(path:&std::path::Path,p:&std::sync::Arc<dyn imdev::vault::Protector>)->String {
 let b=fs::read(Repository::encrypted_path_for(path)).unwrap();String::from_utf8(p.unprotect(&b[b"IMDEV-REQUESTS-V2\0".len()..]).unwrap()).unwrap()
}
#[test] fn known_secrets_are_excluded_even_from_plaintext(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let p=crypto::protector();let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();
 let mut s=snapshot();s.draft.request.url.push_str("&token=SECRET_URL");s.draft.request.headers.push(("Authorization".into(),"SECRET_HEADER".into()));s.draft.request.body=r#"{"password":"SECRET_BODY","ordinary":7}"#.into();
 r.save("safe",&s).unwrap();let text=plain(&path,&p);for secret in ["SECRET_URL","SECRET_HEADER","SECRET_BODY"]{assert!(!text.contains(secret),"{text}");}assert!(text.contains(policy::REDACTED));assert!(text.contains("ordinary"));
}
#[test] fn history_never_stores_response_content_or_error_detail(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let p=crypto::protector();let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();
 let response=imdev::core::ResponseData{status:201,status_text:"SECRET_STATUS".into(),headers:vec![("Set-Cookie".into(),"SECRET_COOKIE".into())],body:"SECRET_RESPONSE".into(),elapsed_ms:17,truncated:false};
 r.record(&snapshot(),&Ok(response)).unwrap();r.record(&snapshot(),&Err("SECRET_ERROR".into())).unwrap();
 let text=plain(&path,&p);for s in ["SECRET_STATUS","SECRET_COOKIE","SECRET_RESPONSE","SECRET_ERROR"]{assert!(!text.contains(s));}assert_eq!(r.history[1].status,Some(201));assert_eq!(r.history[1].elapsed_ms,17);assert!(r.history[0].error);
 let mut reopened=Repository::open_with(path,p).unwrap();assert_eq!(reopened.history.len(),2);reopened.clear_history().unwrap();assert!(reopened.history.is_empty());
}

fn snapshot()->policy::Snapshot {
 let draft=RequestDraft{request:Request{url:"https://example.test/items?x=1&x=2&raw=a%20b".into(),method:"POST".into(),query:vec![("x".into(),"3".into()),("x".into(),"4".into())],headers:vec![("X-Ordinary".into(),"value".into())],body_type:"json".into(),body:"{ \"ordinary\": [1,2] }".into(),timeout_ms:4321},..Default::default()};
 policy::snapshot(&draft,&AuthSession::default()).unwrap()
}
#[test] fn full_request_reopens_with_encrypted_metadata(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let p=crypto::protector();
 let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();let s=snapshot();r.save("general request",&s).unwrap();r.set_auth_origin("https://EXAMPLE.test:443/").unwrap();
 let encrypted=Repository::encrypted_path_for(&path);assert_eq!(encrypted,dir.path().join("state.requests-v2.enc"));
 let bytes=fs::read(&encrypted).unwrap();assert!(!String::from_utf8_lossy(&bytes).contains("general request"));
 let r=Repository::open_with(path,p).unwrap();assert_eq!(r.saved[0].snapshot.draft,s.draft);assert_eq!(r.auth_origin,"https://example.test");
}

#[test] fn session_token_and_user_exclusions_do_not_enter_payload(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let p=crypto::protector();let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();
 let session=AuthSession{origin:"https://example.test".into(),token:"SESSION_SYNTHETIC".into()};
 let mut draft=snapshot().draft;draft.auth=policy::AuthMode::Common;draft.request.url="https://example.test/SESSION_SYNTHETIC?custom=USER_QUERY".into();draft.request.body=r#"{"ordinary":"SESSION_SYNTHETIC","private":"USER_BODY"}"#.into();draft.exclusions.query_keys.push("custom".into());draft.exclusions.json_pointers.push("/private".into());
 let s=policy::snapshot(&draft,&session).unwrap();r.save("clean",&s).unwrap();r.record(&s,&Err("SESSION_SYNTHETIC".into())).unwrap();r.set_auth_origin(&session.origin).unwrap();
 for text in [plain(&path,&p),String::from_utf8_lossy(&fs::read(Repository::encrypted_path_for(&path)).unwrap()).into_owned()] {for secret in ["SESSION_SYNTHETIC","USER_QUERY","USER_BODY"]{assert!(!text.contains(secret));}}
 let reopened=Repository::open_with(path,p).unwrap();assert_eq!(reopened.saved[0].snapshot.draft.auth,policy::AuthMode::Common);assert!(!reopened.saved[0].snapshot.missing.is_empty());
}
#[test] fn whole_field_placeholders_survive_persistence(){
 let d=tempfile::tempdir().unwrap();let p=crypto::protector();let mut r=Repository::open_with(d.path().join("state.json"),p).unwrap();
 let mut draft=snapshot().draft;let session=AuthSession{token:"example".into(),origin:String::new()};draft.request.url="https://example.test/".into();
 let s=policy::snapshot(&draft,&session).unwrap();assert_eq!(s.draft.request.url,policy::REDACTED);r.save("placeholder",&s).unwrap();assert_eq!(r.saved[0].snapshot.draft.request.url,policy::REDACTED);
}
#[test] fn tampering_wrong_key_and_corruption_preserve_original(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let p=crypto::protector();let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();r.save("one",&snapshot()).unwrap();let enc=Repository::encrypted_path_for(&path);let original=fs::read(&enc).unwrap();
 assert!(Repository::open_with(path.clone(),crypto::keyed(9)).is_err());assert_eq!(fs::read(&enc).unwrap(),original);
 let mut corrupt=original.clone();let n=corrupt.len();corrupt[n-1]^=1;fs::write(&enc,&corrupt).unwrap();assert!(Repository::open_with(path.clone(),p.clone()).is_err());assert_eq!(fs::read(&enc).unwrap(),corrupt);
 fs::write(&enc,b"garbage").unwrap();assert!(Repository::open_with(path,p).is_err());assert_eq!(fs::read(enc).unwrap(),b"garbage");
}
#[test] fn legacy_is_read_only_not_migrated_and_corrupt_legacy_is_preserved(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let mut old=imdev::store::Store::open(path.clone()).unwrap();old.save("old",&snapshot().draft.request).unwrap();let original=fs::read(&path).unwrap();
 let mut r=Repository::open_with(path.clone(),crypto::protector()).unwrap();assert!(r.saved.is_empty());assert_eq!(r.legacy_saved.len(),1);assert!(!Repository::encrypted_path_for(&path).exists());r.save("new",&snapshot()).unwrap();assert_eq!(fs::read(&path).unwrap(),original);
 fs::write(&path,b"corrupt legacy").unwrap();assert!(Repository::open_with(path.clone(),crypto::protector()).is_err());assert_eq!(fs::read(path).unwrap(),b"corrupt legacy");
}
struct Switch { inner:std::sync::Arc<dyn imdev::vault::Protector>, fail:std::sync::atomic::AtomicBool }
impl imdev::vault::Protector for Switch {
 fn protect(&self,p:&[u8])->Result<Vec<u8>,String>{if self.fail.load(std::sync::atomic::Ordering::SeqCst){Err("synthetic protection failure".into())}else{self.inner.protect(p)}}
 fn unprotect(&self,c:&[u8])->Result<Vec<u8>,String>{self.inner.unprotect(c)}
}
#[test] fn failed_protection_leaves_memory_disk_and_directory_unchanged(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("private/state.json");let p=std::sync::Arc::new(Switch{inner:crypto::protector(),fail:std::sync::atomic::AtomicBool::new(true)});let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();assert!(r.save("one",&snapshot()).is_err());assert!(!path.parent().unwrap().exists());assert!(r.saved.is_empty());
 p.fail.store(false,std::sync::atomic::Ordering::SeqCst);r.save("one",&snapshot()).unwrap();let enc=Repository::encrypted_path_for(&path);let original=fs::read(enc).unwrap();p.fail.store(true,std::sync::atomic::Ordering::SeqCst);assert!(r.save("two",&snapshot()).is_err());assert!(r.set_auth_origin("https://example.test").is_err());assert_eq!(r.saved.len(),1);assert!(r.auth_origin.is_empty());assert_eq!(fs::read(Repository::encrypted_path_for(&path)).unwrap(),original);assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(),1);
}
#[test] fn failed_write_leaves_memory_and_existing_ciphertext_unchanged(){
 let d=tempfile::tempdir().unwrap();let parent=d.path().join("private");let path=parent.join("state.json");let mut r=Repository::open_with(path.clone(),crypto::protector()).unwrap();r.save("one",&snapshot()).unwrap();
 let moved=d.path().join("moved");fs::rename(&parent,&moved).unwrap();fs::write(&parent,b"directory blocker").unwrap();let original=fs::read(moved.join("state.requests-v2.enc")).unwrap();assert!(r.save("two",&snapshot()).is_err());assert_eq!(r.saved.len(),1);assert_eq!(fs::read(moved.join("state.requests-v2.enc")).unwrap(),original);assert_eq!(fs::read_dir(&moved).unwrap().count(),1);
}
#[test] fn remove_and_limits_are_durable(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let p=crypto::protector();let mut r=Repository::open_with(path.clone(),p.clone()).unwrap();r.save("one",&snapshot()).unwrap();let id=r.saved[0].id.clone();r.remove(&id).unwrap();assert!(Repository::open_with(path.clone(),p.clone()).unwrap().saved.is_empty());
 // Public vectors allow inexpensive boundary construction without hundreds of disk writes.
 r.save("one",&snapshot()).unwrap();r.saved=vec![r.saved[0].clone();200];assert!(r.save("overflow",&snapshot()).is_err());
 r.record(&snapshot(),&Err("not persisted".into())).unwrap();r.history=vec![r.history[0].clone();100];r.record(&snapshot(),&Err("not persisted".into())).unwrap();assert_eq!(r.history.len(),100);
 let before=fs::read(Repository::encrypted_path_for(&path)).unwrap();r.saved=vec![r.saved[0].clone();200];for entry in &mut r.saved {entry.snapshot.draft.request.body_type="text".into();entry.snapshot.draft.request.body="a".repeat(50000);}assert!(r.set_auth_origin("https://example.test").is_err());assert_eq!(fs::read(Repository::encrypted_path_for(&path)).unwrap(),before);
}
#[cfg(unix)]
#[test] fn encrypted_file_and_new_parent_are_private(){
 use std::os::unix::fs::PermissionsExt;let d=tempfile::tempdir().unwrap();let path=d.path().join("private/state.json");let mut r=Repository::open_with(path.clone(),crypto::protector()).unwrap();r.save("one",&snapshot()).unwrap();assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode()&0o777,0o700);assert_eq!(fs::metadata(Repository::encrypted_path_for(&path)).unwrap().permissions().mode()&0o777,0o600);
}
#[cfg(not(windows))]
#[test] fn platform_has_no_plaintext_fallback(){
 use imdev::vault::Protector;let p=imdev::vault::PlatformProtector;assert!(p.protect(b"secret").is_err());assert!(p.unprotect(b"secret").is_err());let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let mut r=Repository::open(path.clone()).unwrap();assert!(r.save("one",&snapshot()).is_err());assert!(!Repository::encrypted_path_for(&path).exists());
}
#[cfg(windows)]
#[test] fn windows_current_user_dpapi_roundtrip(){
 use imdev::vault::Protector;let p=imdev::vault::PlatformProtector;let c=p.protect(b"synthetic DPAPI test").unwrap();assert_ne!(c,b"synthetic DPAPI test");assert_eq!(p.unprotect(&c).unwrap(),b"synthetic DPAPI test");let mut bad=c;bad[0]^=1;assert!(p.unprotect(&bad).is_err());
}
#[test] fn redacted_body_type_and_method_can_be_saved(){
 let d=tempfile::tempdir().unwrap();let mut r=Repository::open_with(d.path().join("state.json"),crypto::protector()).unwrap();
 let draft=RequestDraft{request:Request{url:"https://example.test/".into(),..Default::default()},..Default::default()};
 for token in ["none","GET"] {let s=policy::snapshot(&draft,&AuthSession{token:token.into(),origin:String::new()}).unwrap();r.save("marker",&s).unwrap();assert_eq!(r.saved.last().unwrap().snapshot.draft,s.draft);}
}
#[test] fn oversize_and_overcount_decrypted_state_are_rejected_without_reset(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let enc=Repository::encrypted_path_for(&path);let p=crypto::protector();
 let entry=serde_json::json!({"id":"id","name":"n","snapshot":snapshot()});
 for bytes in [vec![b' ';8*1024*1024+1],serde_json::to_vec(&serde_json::json!({"saved":vec![entry;201],"history":[],"auth_origin":""})).unwrap()] {
  let mut cipher=b"IMDEV-REQUESTS-V2\0".to_vec();cipher.extend(p.protect(&bytes).unwrap());fs::write(&enc,&cipher).unwrap();assert!(Repository::open_with(path.clone(),p.clone()).is_err());assert_eq!(fs::read(&enc).unwrap(),cipher);
 }
}
#[test] fn public_vectors_cannot_bypass_commit_entry_limits(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let mut r=Repository::open_with(path.clone(),crypto::protector()).unwrap();r.save("one",&snapshot()).unwrap();let original=fs::read(Repository::encrypted_path_for(&path)).unwrap();r.saved=vec![r.saved[0].clone();201];assert!(r.set_auth_origin("https://example.test").is_err());assert_eq!(fs::read(Repository::encrypted_path_for(&path)).unwrap(),original);
}
#[test] fn rename_failure_removes_ciphertext_temporary_file(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("state.json");let mut r=Repository::open_with(path.clone(),crypto::protector()).unwrap();r.save("one",&snapshot()).unwrap();let enc=Repository::encrypted_path_for(&path);let original=fs::read(&enc).unwrap();let backup=d.path().join("original.enc");fs::rename(&enc,&backup).unwrap();fs::create_dir(&enc).unwrap();assert!(r.save("two",&snapshot()).is_err());assert_eq!(r.saved.len(),1);assert_eq!(fs::read(backup).unwrap(),original);assert_eq!(fs::read_dir(d.path()).unwrap().count(),2);assert_eq!(fs::read_dir(enc).unwrap().count(),0);
}
