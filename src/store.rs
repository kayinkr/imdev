use crate::core::{Request,ResponseData};
use serde::{Deserialize,Serialize};
use std::{path::PathBuf,fs,io::Write,sync::atomic::{AtomicU64,Ordering},time::{SystemTime,UNIX_EPOCH}};
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()}
fn id()->String{static NEXT:AtomicU64=AtomicU64::new(0);format!("{}-{}-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos(),std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed))}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct SavedEntry{pub id:String,pub name:String,pub request:Request}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct HistoryEntry{pub id:String,pub url:String,pub method:String,pub status:Option<u16>,pub elapsed_ms:u64,pub error:bool,pub at:u64}
#[derive(Serialize,Deserialize,Default)]
struct State{saved:Vec<SavedEntry>,history:Vec<HistoryEntry>}
pub struct Store{path:PathBuf,pub saved:Vec<SavedEntry>,pub history:Vec<HistoryEntry>}
impl Store{
 pub fn default_path()->PathBuf{
 if let Some(path)=std::env::var_os("IMDEV_DATA_DIR"){return PathBuf::from(path).join("state.json");}
 let home=std::env::var_os("HOME").or_else(||std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(||PathBuf::from("."));
 if cfg!(target_os="windows"){std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or(home).join("Imdev").join("state.json")}else if cfg!(target_os="macos"){home.join("Library/Application Support/Imdev/state.json")}else{home.join(".local/share/Imdev/state.json")}
 }
 pub fn open(path:PathBuf)->Result<Self,String>{
 let state=match fs::read(&path){Ok(bytes)=>{if bytes.len()>8*1024*1024{return Err("저장 파일이 허용 크기를 초과했습니다. 원본을 보존하세요.".into());}serde_json::from_slice::<State>(&bytes).map_err(|_|"저장 파일을 읽을 수 없습니다. 원본을 보존한 뒤 확인하세요.".to_string())?},Err(e)if e.kind()==std::io::ErrorKind::NotFound=>State::default(),Err(_)=>return Err("저장 파일을 열 수 없습니다. 권한을 확인하세요.".into())};
 Ok(Self{path,saved:state.saved,history:state.history})
 }
 fn commit(&mut self,saved:Vec<SavedEntry>,history:Vec<HistoryEntry>)->Result<(),String>{
 let data=serde_json::to_vec_pretty(&State{saved:saved.clone(),history:history.clone()}).map_err(|_|"저장 데이터 변환에 실패했습니다.")?;
 if data.len()>8*1024*1024{return Err("저장 데이터 한도 8MiB에 도달했습니다.".into());}
 let parent=self.path.parent().filter(|p|!p.as_os_str().is_empty()).ok_or("저장 경로에 상위 폴더가 필요합니다.")?;
 let mut dir=fs::DirBuilder::new();dir.recursive(true);#[cfg(unix)]{use std::os::unix::fs::DirBuilderExt;dir.mode(0o700);}
 dir.create(parent).map_err(|_|"저장 폴더를 만들 수 없습니다.")?;
 let temp=parent.join(format!(".imdev-{}.tmp",id()));
 let mut options=fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
 let result=(||->std::io::Result<()>{let mut file=options.open(&temp)?;file.write_all(&data)?;file.sync_all()?;drop(file);fs::rename(&temp,&self.path)?;Ok(())})();
 if result.is_err(){let _=fs::remove_file(&temp);return Err("저장에 실패했습니다. 메모리와 기존 저장 파일은 변경하지 않았습니다.".into());}
 self.saved=saved;self.history=history;Ok(())
 }
 pub fn save(&mut self,name:&str,request:&Request)->Result<(),String>{
 if self.saved.len()>=200{return Err("저장 한도 200개에 도달했습니다.".into());}
 if name.trim().is_empty(){return Err("요청 이름을 입력하세요.".into());}
 let clean=request.sanitized()?;clean.validate()?;
 let mut saved=self.saved.clone();saved.push(SavedEntry{id:id(),name:name.trim().chars().take(100).collect(),request:clean});self.commit(saved,self.history.clone())
 }
 pub fn remove(&mut self,id:&str)->Result<(),String>{let saved=self.saved.iter().filter(|x|x.id!=id).cloned().collect();self.commit(saved,self.history.clone())}
 pub fn record(&mut self,request:&Request,result:&Result<ResponseData,String>)->Result<(),String>{
 let origin=url::Url::parse(&request.url).map_err(|_|"잘못된 요청 주소는 기록하지 않습니다.")?.origin().ascii_serialization();
 let mut history=self.history.clone();history.insert(0,HistoryEntry{id:id(),url:origin,method:request.method.clone(),status:result.as_ref().ok().map(|r|r.status),elapsed_ms:result.as_ref().ok().map(|r|r.elapsed_ms).unwrap_or(0),error:result.is_err(),at:now()});history.truncate(100);self.commit(self.saved.clone(),history)
 }
 pub fn clear_history(&mut self)->Result<(),String>{self.commit(self.saved.clone(),vec![])}
}
