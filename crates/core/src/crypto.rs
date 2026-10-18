//! Backup iOS cifrati: keybag (BackupKeyBag in Manifest.plist), chiavi di classe, AES-256-CBC dei file.
//! Schema: password -> PBKDF2-SHA256(DPSL, DPIC) -> PBKDF2-SHA1(SALT, ITER) = chiave che sblocca (RFC 3394)
//! le chiavi di classe; ogni file (e Manifest.db) ha una propria chiave, avvolta con la chiave della sua classe.

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::Aes256;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub type ClassKeys = HashMap<u32, [u8; 32]>;

fn be32(v: &[u8]) -> u32 {
    v.get(..4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).unwrap_or(0)
}

/// Decodifica il keybag TLV (tag 4 byte, lunghezza u32 BE) e sblocca le chiavi di classe con la password.
pub fn unlock(keybag: &[u8], password: &[u8]) -> Result<ClassKeys, String> {
    let (mut attrs, mut classes) = (HashMap::<&[u8], &[u8]>::new(), Vec::<HashMap<&[u8], &[u8]>>::new());
    let (mut top_uuid, mut top_wrap) = (false, false);
    let mut b = keybag;
    while b.len() >= 8 {
        let (tag, len) = (&b[..4], be32(&b[4..8]) as usize);
        let val = b.get(8..8 + len).ok_or("keybag troncato")?;
        b = &b[8 + len..];
        match tag {
            b"UUID" if !top_uuid => top_uuid = true,
            b"WRAP" if !top_wrap => top_wrap = true,
            b"UUID" => classes.push(HashMap::from([(tag, val)])),
            b"CLAS" | b"WRAP" | b"WPKY" | b"KTYP" | b"PBKY" if !classes.is_empty() => {
                classes.last_mut().unwrap().insert(tag, val);
            }
            _ => {
                attrs.insert(tag, val);
            }
        }
    }

    let attr = |k: &[u8]| attrs.get(k).copied().ok_or(format!("keybag senza {}", String::from_utf8_lossy(k)));
    // iOS 10.2+: primo passaggio PBKDF2-SHA256 (DPSL/DPIC); prima esisteva solo il secondo.
    let mut stage1 = password.to_vec();
    if let (Ok(dpsl), Ok(dpic)) = (attr(b"DPSL"), attr(b"DPIC")) {
        let mut k = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password, dpsl, be32(dpic), &mut k);
        stage1 = k.to_vec();
    }
    let mut kek = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&stage1, attr(b"SALT")?, be32(attr(b"ITER")?), &mut kek);

    let mut keys = ClassKeys::new();
    for c in &classes {
        let (Some(wpky), Some(wrap), Some(clas)) = (c.get(&b"WPKY"[..]), c.get(&b"WRAP"[..]), c.get(&b"CLAS"[..])) else {
            continue;
        };
        // WRAP 2 = solo password (backup); 1/3 richiedono la chiave hardware del dispositivo.
        if be32(wrap) == 2 {
            let k = aes_unwrap(&kek, wpky).ok_or("Password errata")?;
            keys.insert(be32(clas), k.get(..32).ok_or("chiave di classe non valida")?.try_into().unwrap());
        }
    }
    if keys.is_empty() {
        return Err("nessuna chiave di classe sbloccabile".into());
    }
    Ok(keys)
}

/// AES Key Unwrap (RFC 3394). None se l'IV di integrità non torna (= chiave/password sbagliata).
pub fn aes_unwrap(kek: &[u8], wrapped: &[u8]) -> Option<Vec<u8>> {
    if wrapped.len() < 24 || wrapped.len() % 8 != 0 {
        return None;
    }
    let aes = Aes256::new_from_slice(kek).ok()?;
    let n = wrapped.len() / 8 - 1;
    let mut a = u64::from_be_bytes(wrapped[..8].try_into().unwrap());
    let mut r: Vec<[u8; 8]> = wrapped[8..].chunks(8).map(|c| c.try_into().unwrap()).collect();
    for j in (0..6).rev() {
        for i in (1..=n).rev() {
            let mut b = [0u8; 16];
            b[..8].copy_from_slice(&(a ^ (n * j + i) as u64).to_be_bytes());
            b[8..].copy_from_slice(&r[i - 1]);
            aes.decrypt_block(GenericArray::from_mut_slice(&mut b));
            a = u64::from_be_bytes(b[..8].try_into().unwrap());
            r[i - 1] = b[8..].try_into().unwrap();
        }
    }
    (a == 0xA6A6_A6A6_A6A6_A6A6).then(|| r.concat())
}

/// Chiave di un file: `wrapped` è avvolta con la chiave della classe di protezione.
pub fn file_key(keys: &ClassKeys, class: u32, wrapped: &[u8]) -> Option<[u8; 32]> {
    aes_unwrap(keys.get(&class)?, wrapped)?.get(..32)?.try_into().ok()
}

/// Lettore AES-256-CBC (IV zero) ad accesso casuale: il blocco i si decifra con D(C[i]) ^ C[i-1],
/// quindi una richiesta Range parte dal blocco giusto senza decifrare tutto il file.
pub struct CbcReader {
    f: File,
    aes: Aes256,
    prev: [u8; 16],
    buf: Vec<u8>,
    pos: usize,
    skip: usize,
    left: u64,
}

impl CbcReader {
    pub fn new(path: &Path, key: &[u8; 32], start: u64, len: u64) -> io::Result<Self> {
        let mut f = File::open(path)?;
        let block = start / 16;
        let mut prev = [0u8; 16];
        if block > 0 {
            f.seek(SeekFrom::Start((block - 1) * 16))?;
            f.read_exact(&mut prev)?;
        }
        Ok(Self { f, aes: Aes256::new(key.into()), prev, buf: vec![], pos: 0, skip: (start % 16) as usize, left: len })
    }
}

impl Read for CbcReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.buf.len() {
            if self.left == 0 {
                return Ok(0);
            }
            let mut chunk = vec![0u8; 256 * 1024];
            let mut n = 0;
            while n < chunk.len() {
                match self.f.read(&mut chunk[n..])? {
                    0 => break,
                    k => n += k,
                }
            }
            chunk.truncate(n - n % 16);
            if chunk.is_empty() {
                return Ok(0);
            }
            for blk in chunk.chunks_exact_mut(16) {
                let c: [u8; 16] = blk.try_into().unwrap();
                self.aes.decrypt_block(GenericArray::from_mut_slice(blk));
                blk.iter_mut().zip(self.prev).for_each(|(p, x)| *p ^= x);
                self.prev = c;
            }
            self.pos = self.skip.min(chunk.len());
            self.skip -= self.pos;
            self.buf = chunk;
        }
        let n = out.len().min(self.buf.len() - self.pos).min(self.left.min(usize::MAX as u64) as usize);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        self.left -= n as u64;
        Ok(n)
    }
}

/// Rimuove il padding PKCS#7 se presente e valido.
pub fn strip_pkcs7(mut v: Vec<u8>) -> Vec<u8> {
    if let Some(&p) = v.last() {
        let p = p as usize;
        if (1..=16).contains(&p) && v.len() >= p && v[v.len() - p..].iter().all(|&b| b as usize == p) {
            v.truncate(v.len() - p);
        }
    }
    v
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use aes::cipher::BlockEncrypt;

    pub fn aes_wrap(kek: &[u8], key: &[u8]) -> Vec<u8> {
        let aes = Aes256::new_from_slice(kek).unwrap();
        let n = key.len() / 8;
        let mut a = 0xA6A6_A6A6_A6A6_A6A6u64;
        let mut r: Vec<[u8; 8]> = key.chunks(8).map(|c| c.try_into().unwrap()).collect();
        for j in 0..6 {
            for i in 1..=n {
                let mut b = [0u8; 16];
                b[..8].copy_from_slice(&a.to_be_bytes());
                b[8..].copy_from_slice(&r[i - 1]);
                aes.encrypt_block(GenericArray::from_mut_slice(&mut b));
                a = u64::from_be_bytes(b[..8].try_into().unwrap()) ^ (n * j + i) as u64;
                r[i - 1] = b[8..].try_into().unwrap();
            }
        }
        [a.to_be_bytes().to_vec(), r.concat()].concat()
    }

    /// AES-256-CBC, IV zero, padding PKCS#7 (come iTunes).
    pub fn cbc_encrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
        let aes = Aes256::new(key.into());
        let p = 16 - data.len() % 16;
        let mut v = data.to_vec();
        v.extend(std::iter::repeat(p as u8).take(p));
        let mut prev = [0u8; 16];
        for blk in v.chunks_exact_mut(16) {
            blk.iter_mut().zip(prev).for_each(|(b, x)| *b ^= x);
            aes.encrypt_block(GenericArray::from_mut_slice(blk));
            prev = blk.try_into().unwrap();
        }
        v
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn rfc3394_vector() {
        // RFC 3394 §4.6: KEK 256 bit, dati 256 bit
        let kek = hex("000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F");
        let data = hex("00112233445566778899AABBCCDDEEFF000102030405060708090A0B0C0D0E0F");
        let wrapped = hex("28C9F404C4B810F4CBCCB35CFB87F8263F5786E2D80ED326CBC7F0E71A99F43BFB988B9B7A02DD21");
        assert_eq!(aes_unwrap(&kek, &wrapped), Some(data.clone()));
        assert_eq!(aes_wrap(&kek, &data), wrapped);
        let mut bad = wrapped.clone();
        bad[10] ^= 1;
        assert_eq!(aes_unwrap(&kek, &bad), None);
    }

    #[test]
    fn cbc_ranges() {
        let key = [7u8; 32];
        let plain: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let path = std::env::temp_dir().join("imancraft-cbc-test.bin");
        std::fs::write(&path, cbc_encrypt(&key, &plain)).unwrap();
        for (s, l) in [(0, plain.len()), (0, 5), (15, 2), (16, 16), (12345, 70_000), (299_990, 10)] {
            let mut out = vec![];
            CbcReader::new(&path, &key, s as u64, l as u64).unwrap().read_to_end(&mut out).unwrap();
            assert_eq!(out, &plain[s..s + l], "range {s}+{l}");
        }
        let _ = std::fs::remove_file(path);
    }
}
