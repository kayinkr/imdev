//! Encrypted request repository; legacy storage is strictly read-only.
use crate::{core::ResponseData, policy::{self, Snapshot}, store, vault::{PlatformProtector, Protector}};
use serde::{Serialize, Deserialize};
use std::{fs, io::Write, path::{Path,PathBuf}, sync::{Arc,atomic::{AtomicU64,Ordering}}, time::{SystemTime,UNIX_EPOCH}};
const MAGIC:&[u8]=b"IMDEV-REQUESTS-V2\0";
const LIMIT:usize=8*1024*1024;
fn now()->u64 {SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
fn id()->String {static NEXT:AtomicU64=AtomicU64::new(0);format!("{}-{}-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed))}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct SavedEntry {pub id:String,pub name:String,pub snapshot:Snapshot}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct HistoryEntry {pub id:String,pub snapshot:Snapshot,pub status:Option<u16>,pub elapsed_ms:u64,pub error:bool,pub at:u64}
#[derive(Default,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct State {saved:Vec<SavedEntry>,history:Vec<HistoryEntry>,auth_origin:String}
pub struct Repository {
 path:PathBuf,protector:Arc<dyn Protector>,pub saved:Vec<SavedEntry>,pub history:Vec<HistoryEntry>,pub auth_origin:String,
 pub legacy_saved:Vec<store::SavedEntry>,pub legacy_history:Vec<store::HistoryEntry>,
}
impl Repository {
 pub fn default_path()->PathBuf {store::Store::default_path()}
 pub fn encrypted_path_for(legacy:&Path)->PathBuf {
  let mut name=legacy.file_stem().unwrap_or_default().to_os_string();name.push(".requests-v2.enc");legacy.with_file_name(name)
 }
 pub fn open(legacy_path:PathBuf)->Result<Self,String> {Self::open_with(legacy_path,Arc::new(PlatformProtector))}
 pub fn open_with(legacy_path:PathBuf,protector:Arc<dyn Protector>)->Result<Self,String> {
  let legacy=store::Store::open(legacy_path.clone())?;
  let path=Self::encrypted_path_for(&legacy_path);
  let state=match fs::read(&path) {
   Ok(bytes)=>{
    let cipher=bytes.strip_prefix(MAGIC).ok_or("Invalid encrypted repository header; original retained.")?;
    let plain=protector.unprotect(cipher)?;
    if plain.len()>LIMIT {return Err("Repository exceeds 8MiB.".into());}
    serde_json::from_slice::<State>(&plain).map_err(|_|"Invalid encrypted repository data; original retained.")?
   },
   Err(e) if e.kind()==std::io::ErrorKind::NotFound=>State::default(),
   Err(_)=>return Err("Cannot read encrypted repository; original retained.".into()),
  };
  if state.saved.len()>200 || state.history.len()>100 {return Err("Repository entry limit exceeded; original retained.".into());}
  if !state.auth_origin.is_empty() {policy::canonical_origin(&state.auth_origin)?;}
  Ok(Self {path,protector,saved:state.saved,history:state.history,auth_origin:state.auth_origin,legacy_saved:legacy.saved,legacy_history:legacy.history})
 }
 fn commit(&mut self,saved:Vec<SavedEntry>,history:Vec<HistoryEntry>,auth_origin:String)->Result<(),String> {
  let mut state=State{saved,history,auth_origin};
  if state.saved.len()>200 || state.history.len()>100 {return Err("Repository entry limit exceeded.".into());}
  if !state.auth_origin.is_empty(){state.auth_origin=policy::canonical_origin(&state.auth_origin)?;}
  for entry in &mut state.saved {entry.snapshot=clean(&entry.snapshot)?;}
  for entry in &mut state.history {entry.snapshot=clean(&entry.snapshot)?;}
  let plain=serde_json::to_vec(&state).map_err(|_|"Cannot serialize repository.")?;
  if plain.len()>LIMIT{return Err("Repository exceeds 8MiB.".into());}
  // Protection must succeed before creating even a directory or temporary file.
  let cipher=self.protector.protect(&plain)?;
  let parent=self.path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
  let mut builder=fs::DirBuilder::new();builder.recursive(true);
  #[cfg(unix)] {use std::os::unix::fs::DirBuilderExt;builder.mode(0o700);}
  builder.create(parent).map_err(|_|"Cannot create repository directory.")?;
  let temp=parent.join(format!(".imdev-encrypted-{}.tmp",id()));
  let mut options=fs::OpenOptions::new();options.create_new(true).write(true);
  #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
  let result=(||->std::io::Result<()> {
   let mut file=options.open(&temp)?;file.write_all(MAGIC)?;file.write_all(&cipher)?;file.sync_all()?;drop(file);
   replace(&temp,&self.path)
  })();
  if result.is_err(){let _=fs::remove_file(&temp);return Err("Cannot persist repository; memory and existing file unchanged.".into());}
  self.saved=state.saved;self.history=state.history;self.auth_origin=state.auth_origin;Ok(())
 }
 pub fn save(&mut self,name:&str,snapshot:&Snapshot)->Result<(),String> {
  if name.trim().is_empty(){return Err("A request name is required.".into());}
  if self.saved.len()>=200{return Err("Saved request limit is 200.".into());}
  let mut saved=self.saved.clone();saved.push(SavedEntry{id:id(),name:name.trim().chars().take(100).collect(),snapshot:snapshot.clone()});
  self.commit(saved,self.history.clone(),self.auth_origin.clone())
 }
 pub fn record(&mut self,snapshot:&Snapshot,result:&Result<ResponseData,String>)->Result<(),String> {
  let mut history=self.history.clone();history.insert(0,HistoryEntry{id:id(),snapshot:snapshot.clone(),status:result.as_ref().ok().map(|r|r.status),elapsed_ms:result.as_ref().ok().map(|r|r.elapsed_ms).unwrap_or(0),error:result.is_err(),at:now()});history.truncate(100);
  self.commit(self.saved.clone(),history,self.auth_origin.clone())
 }
 pub fn remove(&mut self,id:&str)->Result<(),String> {
  let saved=self.saved.iter().filter(|e|e.id!=id).cloned().collect();self.commit(saved,self.history.clone(),self.auth_origin.clone())
 }
 pub fn clear_history(&mut self)->Result<(),String>{self.commit(self.saved.clone(),vec![],self.auth_origin.clone())}
 pub fn set_auth_origin(&mut self,origin:&str)->Result<(),String> {
  let origin=if origin.is_empty(){String::new()}else{policy::canonical_origin(origin)?};
  self.commit(self.saved.clone(),self.history.clone(),origin)
 }
}
fn clean(input:&Snapshot)->Result<Snapshot,String> {
 let mut draft=input.draft.clone();
 // Whole-field redaction can intentionally make an otherwise valid draft
 // unsendable. Use validation surrogates only for exact policy markers.
 let url_marker=draft.request.url==policy::REDACTED;
 let method_marker=draft.request.method==policy::REDACTED;
 let type_marker=draft.request.body_type==policy::REDACTED;
 if url_marker {draft.request.url="https://redacted.invalid/".into();}
 if method_marker {draft.request.method="POST".into();}
 if type_marker {draft.request.body_type="none".into();}
 let mut result=policy::snapshot(&draft,&policy::AuthSession::default())?;
 if url_marker {result.draft.request.url=policy::REDACTED.into();}
 if method_marker {result.draft.request.method=policy::REDACTED.into();}
 if type_marker {result.draft.request.body_type=policy::REDACTED.into();}
 result.draft.auth=draft.auth;
 for location in &input.missing {if !result.missing.contains(location){result.missing.push(location.clone());}}
 Ok(result)
}
#[cfg(not(windows))]
fn replace(from:&Path,to:&Path)->std::io::Result<()> {fs::rename(from,to)}
#[cfg(windows)]
fn replace(from:&Path,to:&Path)->std::io::Result<()> {
 use std::os::windows::ffi::OsStrExt;
 let from:Vec<u16>=from.as_os_str().encode_wide().chain(Some(0)).collect();let to:Vec<u16>=to.as_os_str().encode_wide().chain(Some(0)).collect();
 let ok=unsafe{windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(),to.as_ptr(),windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING|windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH)};
 if ok==0{Err(std::io::Error::last_os_error())}else{Ok(())}
}
