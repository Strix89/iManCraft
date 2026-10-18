//! Recupero password e decifratura forense per Apple Notes protette da password (iOS / iPadOS / macOS).
//! Supporta:
//! 1. Verifica istantanea di password candidate tramite PBKDF2-SHA256 e RFC 3394 AES Key Unwrap (0 false-positive).
//! 2. Decifratura completa AES-GCM (128/256 bit con IV a 12 o 16 byte), decompressione Gzip e parsing Protobuf.
//! 3. Generatore di permutazioni contestuali basate sull'indizio (PasswordHint) e attacco a dizionario multi-core (rayon).

use crate::crypto::aes_unwrap;
use crate::APPLE_EPOCH;
use aes_gcm::aead::consts::U16;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, AesGcm};
use flate2::read::GzDecoder;
use rayon::prelude::*;
use rusqlite::Connection;
use std::collections::HashSet;
use std::io::Read;

type Aes128Gcm16 = AesGcm<aes::Aes128, U16>;
type Aes256Gcm16 = AesGcm<aes::Aes256, U16>;

/// Parametri di verifica dell'account Apple Notes (tabella ZICCLOUDSYNCINGOBJECT PK=2 o simili).
#[derive(Clone, Debug)]
pub struct AccountCryptoVerifier {
    pub salt: Vec<u8>,
    pub iterations: u32,
    pub verifier: Vec<u8>,
    pub hint: Option<String>,
}

/// Dati completi di una nota protetta per il recupero e la decifratura.
#[derive(Clone, Debug)]
pub struct ProtectedNoteData {
    pub note_pk: i64,
    pub title: String,
    pub folder: String,
    pub modified: f64,
    pub snippet: String,
    pub salt: Vec<u8>,
    pub iterations: u32,
    pub wrapped_key: Vec<u8>,
    pub iv: Vec<u8>,
    pub tag: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// Verifica se una password candidata sblocca il verifier (account o singola nota).
/// Ritorna `Some(unwrapped_key)` se la password è corretta, `None` altrimenti.
pub fn verify_candidate(password: &str, salt: &[u8], iterations: u32, verifier_or_wrapped: &[u8]) -> Option<Vec<u8>> {
    let mut kek = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), salt, iterations, &mut kek);
    aes_unwrap(&kek, verifier_or_wrapped)
}

/// Decifra il ciphertext AES-GCM (128/256 bit, IV a 12 o 16 byte) e ne estrae il testo dal protobuf.
pub fn decrypt_note_payload(key: &[u8], iv: &[u8], tag: &[u8], ciphertext: &[u8]) -> Result<String, String> {
    if iv.is_empty() || tag.len() != 16 || ciphertext.is_empty() {
        return Err("Parametri crittografici non validi o incompleti".into());
    }

    let mut combined = ciphertext.to_vec();
    combined.extend_from_slice(tag);

    let payload = Payload {
        msg: &combined,
        aad: &[],
    };

    let decrypted = match (key.len(), iv.len()) {
        (16, 16) => {
            let cipher = Aes128Gcm16::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce = aes_gcm::aead::generic_array::GenericArray::from_slice(iv);
            cipher.decrypt(nonce, payload).map_err(|e| format!("AES-128-GCM (IV 16) fallito: {e}"))?
        }
        (16, 12) => {
            let cipher = Aes128Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce = aes_gcm::Nonce::from_slice(iv);
            cipher.decrypt(nonce, payload).map_err(|e| format!("AES-128-GCM (IV 12) fallito: {e}"))?
        }
        (32, 16) => {
            let cipher = Aes256Gcm16::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce = aes_gcm::aead::generic_array::GenericArray::from_slice(iv);
            cipher.decrypt(nonce, payload).map_err(|e| format!("AES-256-GCM (IV 16) fallito: {e}"))?
        }
        (32, 12) => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
            let nonce = aes_gcm::Nonce::from_slice(iv);
            cipher.decrypt(nonce, payload).map_err(|e| format!("AES-256-GCM (IV 12) fallito: {e}"))?
        }
        _ => return Err(format!("Combinazione chiave ({} byte) e IV ({} byte) non supportata", key.len(), iv.len())),
    };

    // La decompressione Gzip seguita dal parsing del campo protobuf testo
    let mut decompressed = Vec::new();
    let mut decoder = GzDecoder::new(&decrypted[..]);
    if decoder.read_to_end(&mut decompressed).is_ok() {
        if let Some(t) = crate::data::note_text(&decrypted) {
            return Ok(t);
        }
        if let Some(t) = crate::data::note_text(&decompressed) {
            return Ok(t);
        }
    }

    if let Some(t) = crate::data::note_text(&decrypted) {
        return Ok(t);
    }

    // Se non è Gzip protobuf standard, proviamo come stringa UTF-8
    String::from_utf8(decrypted).map_err(|e| format!("Dati decifrati non interpretabili come testo: {e}"))
}

/// Sblocca e decifra una nota protetta usando la password fornita.
pub fn unlock_note(note: &ProtectedNoteData, verifier: Option<&AccountCryptoVerifier>, password: &str) -> Result<String, String> {
    // 1. Prova a ricavare la chiave account
    let mut account_key = None;
    if let Some(v) = verifier {
        account_key = verify_candidate(password, &v.salt, v.iterations, &v.verifier);
    }

    // 2. Se abbiamo la chiave account e la nota ha una wrapped key di 40 o 24 byte:
    if let Some(ref ak) = account_key {
        if note.wrapped_key.len() >= 24 {
            if let Some(note_key) = aes_unwrap(ak, &note.wrapped_key) {
                return decrypt_note_payload(&note_key, &note.iv, &note.tag, &note.ciphertext);
            }
        }
    }

    // 3. Prova diretta con KEK derivata dalla nota (legacy o singola nota):
    let iters = if note.iterations > 0 { note.iterations } else { 20000 };
    let mut note_kek = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &note.salt, iters, &mut note_kek);

    if note.wrapped_key.len() >= 24 {
        if let Some(note_key) = aes_unwrap(&note_kek, &note.wrapped_key) {
            return decrypt_note_payload(&note_key, &note.iv, &note.tag, &note.ciphertext);
        }
    }

    Err("Password non valida per questa nota o firma di autenticazione non corrispondente".into())
}


/// Generatore intelligente di varianti candidate a partire dall'indizio (PasswordHint).
pub fn generate_hint_candidates(hint: &str) -> Vec<String> {
    let mut set = HashSet::new();
    let trimmed = hint.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // Aggiungi la stringa esatta e varianti dirette
    set.insert(trimmed.to_string());
    set.insert(trimmed.to_lowercase());
    set.insert(trimmed.to_uppercase());

    // Senza punteggiatura
    let no_punct: String = trimmed.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    set.insert(no_punct.clone());
    set.insert(no_punct.to_lowercase());

    // Senza spazi
    let no_spaces: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    set.insert(no_spaces.clone());
    set.insert(no_spaces.to_lowercase());

    let no_spaces_punct: String = no_punct.chars().filter(|c| !c.is_whitespace()).collect();
    set.insert(no_spaces_punct.clone());
    set.insert(no_spaces_punct.to_lowercase());

    // Sostituzioni accenti (es. 'è' -> 'e', 'è' -> "e'")
    let replaced_e = trimmed.replace('è', "e").replace('é', "e").replace('à', "a").replace('ò', "o").replace('ù', "u").replace('ì', "i");
    set.insert(replaced_e.clone());
    set.insert(replaced_e.to_lowercase());
    let replaced_e_no_punct: String = replaced_e.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    set.insert(replaced_e_no_punct.clone());
    set.insert(replaced_e_no_punct.to_lowercase());
    let replaced_e_no_spaces: String = replaced_e_no_punct.chars().filter(|c| !c.is_whitespace()).collect();
    set.insert(replaced_e_no_spaces.clone());
    set.insert(replaced_e_no_spaces.to_lowercase());

    // Singole parole
    let words: Vec<&str> = no_punct.split_whitespace().collect();
    for w in &words {
        set.insert(w.to_string());
        set.insert(w.to_lowercase());
        set.insert(capitalize(w));
    }

    // Suffissi comuni
    let base_list: Vec<String> = set.iter().cloned().collect();
    for b in &base_list {
        for suffix in &["1", "12", "123", "1234", "!", "?", ".", "2020", "2021", "2022", "2023", "2024", "2025", "2026"] {
            set.insert(format!("{b}{suffix}"));
        }
    }

    let mut out: Vec<String> = set.into_iter().collect();
    out.sort_by_key(|s| s.len());
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[derive(Clone, Debug)]
pub enum CrackProgress {
    Running { tried: usize, total: usize, speed: f32 },
    Found { password: String, tried: usize },
    DoneNotFound { total: usize },
    Cancelled,
}

pub fn generate_pin_candidates() -> Vec<String> {
    let mut v = Vec::with_capacity(10000 + 20);
    for i in 0..=9999 {
        v.push(format!("{:04}", i));
    }
    for i in 0..=9 {
        v.push(format!("{i}{i}{i}{i}{i}{i}"));
    }
    for pin in ["123456", "654321", "123123", "112233", "121212", "012345", "135790", "246800"] {
        v.push(pin.to_string());
    }
    v
}

pub fn common_passwords() -> Vec<String> {
    vec![
        "password", "123456", "12345678", "1234", "12345", "qwerty", "123456789", "111111",
        "1234567", "dragon", "123123", "baseball", "football", "shadow", "master", "monkey",
        "access", "secret", "mustang", "superman", "apple", "iloveyou", "admin", "welcome",
        "italia", "roma", "milan", "juventus", "napoli", "inter", "amore", "ciao", "sole",
    ].into_iter().map(String::from).collect()
}

/// Esegue una sessione di recupero su thread di lavoro riportando il progresso a blocchi.
pub fn run_crack_session(
    salt: Vec<u8>,
    iterations: u32,
    verifier_or_wrapped: Vec<u8>,
    candidates: Vec<String>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    sender: std::sync::mpsc::Sender<CrackProgress>,
) {
    let total = candidates.len();
    let batch_size = 200.max(total / 50).min(1000);
    let mut tried = 0;
    let start_time = std::time::Instant::now();

    for chunk in candidates.chunks(batch_size) {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = sender.send(CrackProgress::Cancelled);
            return;
        }

        let found = chunk.par_iter().find_any(|cand| {
            verify_candidate(cand, &salt, iterations, &verifier_or_wrapped).is_some()
        });

        if let Some(pwd) = found {
            tried += chunk.iter().position(|c| c == pwd).unwrap_or(0) + 1;
            let _ = sender.send(CrackProgress::Found {
                password: pwd.clone(),
                tried,
            });
            return;
        }

        tried += chunk.len();
        let elapsed = start_time.elapsed().as_secs_f32().max(0.001);
        let speed = tried as f32 / elapsed;
        let _ = sender.send(CrackProgress::Running { tried, total, speed });
    }

    let _ = sender.send(CrackProgress::DoneNotFound { total: tried });
}

/// Recupera la password provando una lista di candidati in parallelo tramite `rayon`.
pub fn crack_parallel(salt: &[u8], iterations: u32, verifier_or_wrapped: &[u8], candidates: &[String]) -> Option<String> {
    candidates
        .par_iter()
        .find_any(|cand| verify_candidate(cand, salt, iterations, verifier_or_wrapped).is_some())
        .cloned()
}


/// Estrae l'AccountCryptoVerifier e l'elenco delle note protette da un database NoteStore.sqlite.
pub fn load_notes_crypto_context(db: &Connection) -> (Option<AccountCryptoVerifier>, Vec<ProtectedNoteData>) {
    let mut verifier: Option<AccountCryptoVerifier> = None;

    // 1. Cerca il verifier nell'account (ZUNAPPLIEDENCRYPTEDRECORDDATA di PK=2 o record simili)
    if let Ok(mut st) = db.prepare("SELECT Z_PK, ZUNAPPLIEDENCRYPTEDRECORDDATA, ZCRYPTOVERIFIER, ZCRYPTOSALT, ZCRYPTOITERATIONCOUNT, ZPASSWORDHINT FROM ZICCLOUDSYNCINGOBJECT WHERE ZUNAPPLIEDENCRYPTEDRECORDDATA IS NOT NULL OR ZCRYPTOVERIFIER IS NOT NULL") {
        if let Ok(mut rows) = st.query([]) {
            while let Ok(Some(r)) = rows.next() {
                // Prova dai byte grezzi di ZUNAPPLIEDENCRYPTEDRECORDDATA
                if let Ok(Some(blob)) = r.get::<_, Option<Vec<u8>>>(1) {
                    if let Some(v) = extract_verifier_from_bytes(&blob) {
                        verifier = Some(v);
                        break;
                    }
                }
                // Altrimenti colonne dirette
                let salt: Option<Vec<u8>> = r.get(3).ok().flatten();
                let verf: Option<Vec<u8>> = r.get(2).ok().flatten();
                let iters: u32 = r.get(4).unwrap_or(0);
                let hint: Option<String> = r.get(5).ok().flatten();
                if let (Some(s), Some(v)) = (salt, verf) {
                    if iters > 0 {
                        verifier = Some(AccountCryptoVerifier {
                            salt: s,
                            iterations: iters,
                            verifier: v,
                            hint,
                        });
                        break;
                    }
                }
            }
        }
    }

    // 2. Estrai tutte le note protette
    let mut protected_notes = Vec::new();
    let query = "
        SELECT n.Z_PK, n.ZTITLE1, f.ZTITLE2, n.ZMODIFICATIONDATE1, n.ZSNIPPET,
               n.ZCRYPTOSALT, n.ZCRYPTOITERATIONCOUNT, n.ZCRYPTOWRAPPEDKEY,
               d.ZCRYPTOINITIALIZATIONVECTOR, d.ZCRYPTOTAG, d.ZDATA
        FROM ZICCLOUDSYNCINGOBJECT n
        JOIN ZICNOTEDATA d ON d.ZNOTE = n.Z_PK
        LEFT JOIN ZICCLOUDSYNCINGOBJECT f ON f.Z_PK = n.ZFOLDER
        WHERE n.ZISPASSWORDPROTECTED = 1 AND coalesce(n.ZMARKEDFORDELETION, 0) = 0
        ORDER BY n.ZMODIFICATIONDATE1 DESC
    ";

    if let Ok(mut st) = db.prepare(query) {
        if let Ok(rows) = st.query_map([], |r| {
            let note_pk: i64 = r.get(0)?;
            let title: String = r.get::<_, Option<String>>(1)?.unwrap_or_default();
            let folder: String = r.get::<_, Option<String>>(2)?.unwrap_or_default();
            let mod_date: f64 = r.get::<_, Option<f64>>(3)?.unwrap_or(0.0) + APPLE_EPOCH;
            let snippet: String = r.get::<_, Option<String>>(4)?.unwrap_or_default();
            let salt: Vec<u8> = r.get::<_, Option<Vec<u8>>>(5)?.unwrap_or_default();
            let iters: u32 = r.get::<_, Option<u32>>(6)?.unwrap_or(0);
            let mut wrapped_key: Vec<u8> = r.get::<_, Option<Vec<u8>>>(7)?.unwrap_or_default();
            let iv: Vec<u8> = r.get::<_, Option<Vec<u8>>>(8)?.unwrap_or_default();
            let tag: Vec<u8> = r.get::<_, Option<Vec<u8>>>(9)?.unwrap_or_default();
            let zdata: Vec<u8> = r.get::<_, Option<Vec<u8>>>(10)?.unwrap_or_default();

            // Se ZDATA contiene un oggetto cifrato (wrapped key / encrypted payload)
            let (final_iv, final_tag, ciphertext) = if zdata.starts_with(b"bplist") {
                if let Some((parsed_wkey, parsed_data)) = parse_crypto_encryption_object(&zdata) {
                    if !parsed_wkey.is_empty() {
                        wrapped_key = parsed_wkey;
                    }
                    (iv, tag, parsed_data)
                } else {
                    (iv, tag, zdata)
                }
            } else {
                (iv, tag, zdata)
            };

            Ok(ProtectedNoteData {
                note_pk,
                title,
                folder,
                modified: mod_date,
                snippet,
                salt,
                iterations: iters,
                wrapped_key,
                iv: final_iv,
                tag: final_tag,
                ciphertext,
            })
        }) {
            for nr in rows.flatten() {
                protected_notes.push(nr);
            }
        }
    }

    (verifier, protected_notes)
}

fn extract_blob_field(blob: &[u8], marker: &[u8], expected_len: usize) -> Option<Vec<u8>> {
    let pos = blob.windows(marker.len()).position(|w| w == marker)?;
    let window = &blob[pos..blob.len().min(pos + 300)];
    let header = [0x4Fu8, 0x10, expected_len as u8];
    let hpos = window.windows(3).position(|w| w == header)?;
    let start = pos + hpos + 3;
    if start + expected_len <= blob.len() {
        Some(blob[start..start + expected_len].to_vec())
    } else {
        None
    }
}

fn extract_verifier_from_bytes(blob: &[u8]) -> Option<AccountCryptoVerifier> {
    let salt = extract_blob_field(blob, b"CryptoSalt", 16)?;
    let verifier = extract_blob_field(blob, b"CryptoVerifier", 24)?;

    let mut iterations = 20000u32;
    if let Some(pos) = blob.windows(20).position(|w| w == b"CryptoIterationCount") {
        let window = &blob[pos..blob.len().min(pos + 100)];
        if let Some(ipos) = window.windows(3).position(|w| w[0] == 0x11) {
            let n = u16::from_be_bytes([window[ipos + 1], window[ipos + 2]]) as u32;
            if n > 1000 {
                iterations = n;
            }
        }
    }

    let mut hint = None;
    if let Some(pos) = blob.windows(12).position(|w| w == b"PasswordHint") {
        let window = &blob[pos..blob.len().min(pos + 200)];
        for i in 0..window.len().saturating_sub(3) {
            if window[i] == 0x4F && window[i + 1] == 0x10 {
                let hlen = window[i + 2] as usize;
                let start = pos + i + 3;
                if start + hlen <= blob.len() {
                    if let Ok(s) = std::str::from_utf8(&blob[start..start + hlen]) {
                        if !s.is_empty() && !s.starts_with("bplist") {
                            hint = Some(s.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    Some(AccountCryptoVerifier {
        salt,
        iterations,
        verifier,
        hint,
    })
}

fn parse_crypto_encryption_object(zdata: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut wrapped_key = Vec::new();
    let mut encrypted_data = Vec::new();

    // 40-byte wrapped key: 0x4F, 0x10, 0x28
    if let Some(pos) = zdata.windows(3).position(|w| w == [0x4F, 0x10, 0x28]) {
        let start = pos + 3;
        if start + 40 <= zdata.len() {
            wrapped_key = zdata[start..start + 40].to_vec();
        }
    } else if let Some(pos) = zdata.windows(3).position(|w| w == [0x4F, 0x10, 0x18]) {
        // 24-byte wrapped key: 0x4F, 0x10, 0x18
        let start = pos + 3;
        if start + 24 <= zdata.len() {
            wrapped_key = zdata[start..start + 24].to_vec();
        }
    }

    // Dati cifrati grandi (lunghezza a 2 byte: 0x4F, 0x11, len_hi, len_lo)
    if let Some(pos) = zdata.windows(2).position(|w| w == [0x4F, 0x11]) {
        if pos + 4 <= zdata.len() {
            let dlen = u16::from_be_bytes([zdata[pos + 2], zdata[pos + 3]]) as usize;
            let start = pos + 4;
            if start + dlen <= zdata.len() {
                encrypted_data = zdata[start..start + dlen].to_vec();
            }
        }
    }

    if !encrypted_data.is_empty() {
        Some((wrapped_key, encrypted_data))
    } else {
        None
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn test_hint_generator() {
        let hint = "nulla è reale, tutto è lecito";
        let cands = generate_hint_candidates(hint);
        assert!(cands.contains(&"nulla è reale, tutto è lecito".to_string()));
        assert!(cands.contains(&"nulla".to_string()));
        assert!(cands.contains(&"reale".to_string()));
        assert!(cands.contains(&"lecito".to_string()));
        assert!(cands.contains(&"nulla e reale tutto e lecito".to_string()));
        assert!(cands.contains(&"nulla123".to_string()));
        assert!(cands.len() > 20);
    }

    #[test]
    fn test_verify_hashcat_vector() {
        let salt = hex("80771171105233481004850004085037");
        let wrapped_key = hex("d04b17af7f6b184346aad3efefe8bec0987ee73418291a41");
        assert!(verify_candidate("hashcat", &salt, 20000, &wrapped_key).is_some());
        assert!(verify_candidate("wrong", &salt, 20000, &wrapped_key).is_none());
    }

    #[test]
    fn test_crack_parallel() {
        let salt = hex("80771171105233481004850004085037");
        let wrapped_key = hex("d04b17af7f6b184346aad3efefe8bec0987ee73418291a41");
        let candidates = vec![
            "password123".into(),
            "admin".into(),
            "hashcat".into(),
            "secret".into(),
        ];
        let found = crack_parallel(&salt, 20000, &wrapped_key, &candidates);
        assert_eq!(found, Some("hashcat".into()));
    }

    #[test]
    fn test_decrypt_payload() {
        let key = [0x42u8; 16];
        let iv = [0x11u8; 16];
        let plaintext = b"Nota segreta test iManCraft 2026";

        // Cifra con Aes128Gcm16
        let cipher = Aes128Gcm16::new_from_slice(&key).unwrap();
        let nonce = aes_gcm::aead::generic_array::GenericArray::from_slice(&iv);
        let ct_with_tag = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
        let ct = &ct_with_tag[..ct_with_tag.len() - 16];
        let tag = &ct_with_tag[ct_with_tag.len() - 16..];

        let dec = decrypt_note_payload(&key, &iv, tag, ct).expect("Decifratura payload fallita");
        assert_eq!(dec, "Nota segreta test iManCraft 2026");
    }

    #[test]
    fn test_real_backup_context_loading() {
        let db_path = r"F:\DumpIOSPing1\Backup\00008110-000A648136D0A01E\4f\4f98687d8ab0d6d1a371110e6b7300f6e465bef2";
        if !std::path::Path::new(db_path).exists() { return; }
        let conn = crate::open_db(std::path::Path::new(db_path)).unwrap();

        let (verifier, notes) = load_notes_crypto_context(&conn);
        assert!(verifier.is_some(), "Verifier non trovato!");
        let v = verifier.unwrap();
        println!("Account verifier loaded successfully:");
        println!("  Salt: {:02x?}", v.salt);
        println!("  Iters: {}", v.iterations);
        println!("  Verifier: {:02x?}", v.verifier);
        println!("  Hint: {:?}", v.hint);
        assert_eq!(v.salt.len(), 16);
        assert_eq!(v.verifier.len(), 24);
        assert_eq!(v.iterations, 20000);

        println!("Loaded {} protected notes:", notes.len());
        assert_eq!(notes.len(), 6);
        for n in &notes {
            println!("  PK {}: '{}' (folder: '{}') - wkey {}b, iv {}b, tag {}b, ct {}b",
                n.note_pk, n.title, n.folder, n.wrapped_key.len(), n.iv.len(), n.tag.len(), n.ciphertext.len());

            assert!(n.wrapped_key.len() == 40 || n.wrapped_key.len() == 16);
            assert_eq!(n.tag.len(), 16);
            assert_eq!(n.iv.len(), 16);
            assert!(!n.ciphertext.is_empty());


        }
    }
}




