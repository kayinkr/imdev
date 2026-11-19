use std::sync::Arc;
use imdev::vault::Protector;
use ring::{aead, rand::{SecureRandom, SystemRandom}};
// Synthetic test-only key. Never imported by application code.
pub fn protector() -> Arc<dyn Protector> { keyed(0x53) }
pub fn keyed(byte:u8) -> Arc<dyn Protector> { Arc::new(TestAead([byte;32])) }
struct TestAead([u8;32]);
impl Protector for TestAead {
 fn protect(&self, plain:&[u8])->Result<Vec<u8>,String>{
  let key=aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM,&self.0).map_err(|_|"key")?);
  let mut nonce=[0;12];SystemRandom::new().fill(&mut nonce).map_err(|_|"random")?;
  let mut out=plain.to_vec();key.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::empty(),&mut out).map_err(|_|"seal")?;
  Ok([nonce.to_vec(),out].concat())
 }
 fn unprotect(&self, cipher:&[u8])->Result<Vec<u8>,String>{
  if cipher.len()<28{return Err("short ciphertext".into());}
  let key=aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM,&self.0).map_err(|_|"key")?);
  let nonce:[u8;12]=cipher[..12].try_into().unwrap();let mut out=cipher[12..].to_vec();
  Ok(key.open_in_place(aead::Nonce::assume_unique_for_key(nonce),aead::Aad::empty(),&mut out).map_err(|_|"authentication failed")?.to_vec())
 }
}
