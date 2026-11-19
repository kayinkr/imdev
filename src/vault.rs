//! OS-bound current-user protection. No plaintext fallback exists.
pub trait Protector: Send + Sync {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, String>;
    fn unprotect(&self, cipher: &[u8]) -> Result<Vec<u8>, String>;
}
pub struct PlatformProtector;
#[cfg(windows)]
impl Protector for PlatformProtector {
    fn protect(&self, plain:&[u8])->Result<Vec<u8>,String> { dpapi(plain,true) }
    fn unprotect(&self, cipher:&[u8])->Result<Vec<u8>,String> { dpapi(cipher,false) }
}
#[cfg(windows)]
fn dpapi(bytes:&[u8],protect:bool)->Result<Vec<u8>,String> {
    use std::ptr::{null,null_mut};
    use windows_sys::Win32::{Foundation::LocalFree,Security::Cryptography::{CRYPT_INTEGER_BLOB,CryptProtectData,CryptUnprotectData,CRYPTPROTECT_UI_FORBIDDEN}};
    let length=u32::try_from(bytes.len()).map_err(|_|"DPAPI input too large.")?;
    let input=CRYPT_INTEGER_BLOB{cbData:length,pbData:bytes.as_ptr() as *mut u8};
    let mut output=CRYPT_INTEGER_BLOB{cbData:0,pbData:null_mut()};
    // No LOCAL_MACHINE flag: DPAPI binds to the logged-in Windows user.
    // Null prompt, UI_FORBIDDEN, no optional entropy or persisted description.
    let ok=unsafe {if protect {CryptProtectData(&input,null(),null(),null(),null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}else{CryptUnprotectData(&input,null_mut(),null(),null(),null(),CRYPTPROTECT_UI_FORBIDDEN,&mut output)}};
    struct Buffer(*mut u8,usize);
    impl Drop for Buffer {fn drop(&mut self){if !self.0.is_null(){unsafe {
        // Clear the API buffer before releasing its LocalAlloc allocation.
        for i in 0..self.1 {std::ptr::write_volatile(self.0.add(i),0);}
        LocalFree(self.0.cast());
    }}}}
    let buffer=Buffer(output.pbData,output.cbData as usize);
    if ok==0 {return Err("Windows current-user DPAPI failed; original data retained.".into());}
    if buffer.0.is_null() {return if buffer.1==0 {Ok(Vec::new())}else{Err("Invalid DPAPI output.".into())};}
    Ok(unsafe{std::slice::from_raw_parts(buffer.0,buffer.1)}.to_vec())
}
#[cfg(not(windows))]
impl Protector for PlatformProtector {
    fn protect(&self, _: &[u8]) -> Result<Vec<u8>, String> { Err("Windows current-user DPAPI is unsupported on this platform.".into()) }
    fn unprotect(&self, _: &[u8]) -> Result<Vec<u8>, String> { Err("Windows current-user DPAPI is unsupported on this platform.".into()) }
}
