//! WinZip AE-2 AES-256 の書き込み。パスワードは UTF-8 バイト。

use std::io::{self, Write};

use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes256;
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use zeroize::{Zeroize, Zeroizing};

pub const AES_EXTRA_ID: u16 = 0x9901;
pub const METHOD_AES: u16 = 99;
pub const GPBF_ENCRYPT: u16 = 0x0001;
pub const VERSION_NEEDED_AES: u16 = 51;

const AES_VENDOR_VERSION_AE2: u16 = 2;
const AES_STRENGTH_256: u8 = 3;
const SALT_LEN: usize = 16;
const KEY_LEN: usize = 32;
const PWD_VERIFY_LEN: usize = 2;
const AUTH_CODE_LEN: usize = 10;
const PBKDF2_ROUNDS: u32 = 1000;

type HmacSha1 = Hmac<Sha1>;

pub fn aes_extra_field(actual_method: u16) -> Vec<u8> {
    let mut extra = Vec::with_capacity(11);
    extra.extend_from_slice(&AES_EXTRA_ID.to_le_bytes());
    extra.extend_from_slice(&7u16.to_le_bytes());
    extra.extend_from_slice(&AES_VENDOR_VERSION_AE2.to_le_bytes());
    extra.extend_from_slice(b"AE");
    extra.push(AES_STRENGTH_256);
    extra.extend_from_slice(&actual_method.to_le_bytes());
    extra
}

pub fn extra_is_ae2_aes256(extra: &[u8]) -> bool {
    let mut i = 0;
    while i + 4 <= extra.len() {
        let tag = u16::from_le_bytes([extra[i], extra[i + 1]]);
        let size = u16::from_le_bytes([extra[i + 2], extra[i + 3]]) as usize;
        i += 4;
        if i + size > extra.len() {
            return false;
        }
        if tag == AES_EXTRA_ID {
            if size < 7 {
                return false;
            }
            let version = u16::from_le_bytes([extra[i], extra[i + 1]]);
            let vendor = &extra[i + 2..i + 4];
            let strength = extra[i + 4];
            return version == AES_VENDOR_VERSION_AE2
                && vendor == b"AE"
                && strength == AES_STRENGTH_256;
        }
        i += size;
    }
    false
}

pub struct Aes256Writer<W> {
    inner: Option<W>,
    cipher: Aes256,
    counter: [u8; 16],
    xor: [u8; 16],
    xor_off: usize,
    hmac: Option<HmacSha1>,
    header: Option<([u8; SALT_LEN], [u8; PWD_VERIFY_LEN])>,
}

impl<W: Write> Aes256Writer<W> {
    pub fn new(inner: W, password: &[u8]) -> io::Result<Self> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::getrandom(&mut salt).map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Self::with_salt(inner, password, salt))
    }

    pub fn with_salt(inner: W, password: &[u8], salt: [u8; SALT_LEN]) -> Self {
        let mut derived = Zeroizing::new([0u8; KEY_LEN * 2 + PWD_VERIFY_LEN]);
        pbkdf2_hmac::<Sha1>(password, &salt, PBKDF2_ROUNDS, derived.as_mut());
        let cipher = Aes256::new_from_slice(&derived[..KEY_LEN]).expect("AES-256 key is 32 bytes");
        let hmac =
            <HmacSha1 as Mac>::new_from_slice(&derived[KEY_LEN..KEY_LEN * 2]).expect("HMAC key");
        let mut pwd_verify = [0u8; PWD_VERIFY_LEN];
        pwd_verify.copy_from_slice(&derived[KEY_LEN * 2..]);
        derived.zeroize();
        Self {
            inner: Some(inner),
            cipher,
            counter: [0; 16],
            xor: [0; 16],
            xor_off: 16,
            hmac: Some(hmac),
            header: Some((salt, pwd_verify)),
        }
    }

    pub fn finish(mut self) -> io::Result<W> {
        self.write_header()?;
        let hmac = self
            .hmac
            .take()
            .ok_or_else(|| io::Error::other("AES writer already finished"))?;
        let code = Mac::finalize(hmac).into_bytes();
        self.inner_mut()?.write_all(&code[..AUTH_CODE_LEN])?;
        self.inner
            .take()
            .ok_or_else(|| io::Error::other("AES writer already finished"))
    }

    fn inner_mut(&mut self) -> io::Result<&mut W> {
        self.inner
            .as_mut()
            .ok_or_else(|| io::Error::other("AES writer already finished"))
    }

    fn write_header(&mut self) -> io::Result<()> {
        if let Some((salt, pwd_verify)) = self.header.take() {
            self.inner_mut()?.write_all(&salt)?;
            self.inner_mut()?.write_all(&pwd_verify)?;
        }
        Ok(())
    }

    fn crypt_in_place(&mut self, buf: &mut [u8]) {
        for byte in buf.iter_mut() {
            if self.xor_off == 16 {
                increment_le64(&mut self.counter);
                let mut block = GenericArray::clone_from_slice(&self.counter);
                self.cipher.encrypt_block(&mut block);
                self.xor.copy_from_slice(&block);
                self.xor_off = 0;
            }
            *byte ^= self.xor[self.xor_off];
            self.xor_off += 1;
        }
    }
}

impl<W: Write> Write for Aes256Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_header()?;
        let mut tmp = Zeroizing::new(buf.to_vec());
        self.crypt_in_place(&mut tmp);
        self.hmac
            .as_mut()
            .ok_or_else(|| io::Error::other("AES writer already finished"))?
            .update(&tmp);
        self.inner_mut()?.write_all(&tmp)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner_mut()?.flush()
    }
}

impl<W> Drop for Aes256Writer<W> {
    fn drop(&mut self) {
        self.hmac.take();
        self.counter.zeroize();
        self.xor.zeroize();
    }
}

fn increment_le64(counter: &mut [u8; 16]) {
    for byte in counter.iter_mut().take(8) {
        let (next, overflow) = byte.overflowing_add(1);
        *byte = next;
        if !overflow {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_field_is_ae2_aes256_deflate() {
        let extra = aes_extra_field(8);
        assert_eq!(extra.len(), 11);
        assert_eq!(&extra[0..2], &AES_EXTRA_ID.to_le_bytes());
        assert_eq!(&extra[2..4], &7u16.to_le_bytes());
        assert_eq!(&extra[4..6], &2u16.to_le_bytes());
        assert_eq!(&extra[6..8], b"AE");
        assert_eq!(extra[8], 3);
        assert_eq!(&extra[9..11], &8u16.to_le_bytes());
        assert!(extra_is_ae2_aes256(&extra));
        assert!(!extra_is_ae2_aes256(&[]));
    }

    #[test]
    fn increment_starts_at_one() {
        let mut c = [0u8; 16];
        increment_le64(&mut c);
        assert_eq!(c[0], 1);
        assert!(c[1..].iter().all(|&b| b == 0));
    }
}
