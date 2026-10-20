//! IManCraft core: lettura dei backup iOS (formato iTunes/Finder/iMazing: Manifest.db + file con hash SHA1), anche cifrati.
//! Nessuna UI qui: l'app (crate `app`) chiama queste funzioni da thread di lavoro.

pub mod calendars;
pub mod crypto;
pub mod data;
pub mod datasets;
pub mod export;
pub mod extractor;
pub mod imazing_app;
pub mod imazing_versions;
pub mod media;
pub mod recordings;
pub mod reminders;
pub mod safari_bookmarks;
pub mod search;
pub mod spyware;
pub mod telegram;
pub mod voicemail;
pub mod whatsapp;
pub mod notes_recovery;



use crypto::{CbcReader, ClassKeys};
use rusqlite::{ffi, serialize::OwnedData, types::ValueRef, Connection, DatabaseName, OpenFlags, Row};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Mutex;

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Errore restituito da [`Backup::with`] per un backup cifrato non ancora sbloccato.
pub const LOCKED: &str = "locked";
pub(crate) const APPLE_EPOCH: f64 = 978_307_200.0; // 2001-01-01 in Unix time

/// Un backup su disco. Va condiviso tra thread con `Arc`.
pub struct Backup {
    pub dir: PathBuf,
    pub encrypted: bool,
    pub summary: Summary,
    /// Chiavi e Manifest.db decifrato: solo in memoria, dopo [`Backup::unlock`].
    unlocked: Mutex<Option<Unlocked>>,
}

struct Unlocked {
    keys: ClassKeys,
    manifest: Connection,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub name: String,
    pub model: String,
    pub ios: String,
    pub date: String,
    pub udid: String,
    pub size: u64,
    pub ddna: Option<datasets::DdnaMeta>,
}


/// Cerca i backup: ogni radice può essere un backup (contiene Manifest.db) o una cartella di backup.
pub fn discover(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = vec![];
    for r in roots {
        if r.join("Manifest.db").exists() {
            out.push(r.clone());
        } else if let Ok(rd) = std::fs::read_dir(r) {
            out.extend(rd.flatten().map(|e| e.path()).filter(|p| p.join("Manifest.db").exists()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Cartelle standard di backup per iTunes, Apple Devices e libimobiledevice.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(windows)]
    {
        for (var, sub) in [("APPDATA", r"Apple Computer\MobileSync\Backup"), ("USERPROFILE", r"Apple\MobileSync\Backup")] {
            if let Ok(b) = std::env::var(var) {
                let p = Path::new(&b).join(sub);
                if p.exists() {
                    roots.push(p);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let p = Path::new(&home).join("Library/Application Support/MobileSync/Backup");
            if p.exists() {
                roots.push(p);
            }
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let p = Path::new(&home);
            // Percorsi standard usati da libimobiledevice (idevicebackup / idevicebackup2) o cartelle importate
            for candidate in [
                p.join(".local/share/libimobiledevice/backup"),
                p.join("Apple/MobileSync/Backup"),
                p.join("MobileSync/Backup"),
                p.join(".local/share/MobileSync/Backup"),
            ] {
                if candidate.exists() {
                    roots.push(candidate);
                }
            }
        }
    }

    roots
}

impl Backup {
    pub fn open(dir: PathBuf) -> Backup {
        let encrypted = read_plist(&dir.join("Manifest.plist"))
            .and_then(|v| v.as_dictionary()?.get("IsEncrypted")?.as_boolean())
            .unwrap_or(false);
        let summary = summary(&dir);
        Backup { dir, encrypted, summary, unlocked: Mutex::new(None) }
    }

    pub fn is_locked(&self) -> bool {
        self.encrypted && self.unlocked.lock().unwrap_or_else(|e| e.into_inner()).is_none()
    }

    /// Esegue `f` con il backup aperto. Per i cifrati serializza l'accesso (Manifest in memoria condiviso).
    pub fn with<T>(&self, f: impl FnOnce(&Ctx) -> Res<T>) -> Res<T> {
        if self.encrypted {
            let guard = self.unlocked.lock().unwrap_or_else(|e| e.into_inner());
            let u = guard.as_ref().ok_or(LOCKED)?;
            f(&Ctx { dir: &self.dir, m: &u.manifest, keys: Some(&u.keys) })
        } else {
            let m = open_db(&self.dir.join("Manifest.db"))?;
            f(&Ctx { dir: &self.dir, m: &m, keys: None })
        }
    }

    /// Sblocca un backup cifrato con la password (PBKDF2: qualche secondo).
    pub fn unlock(&self, password: &str) -> Res<()> {
        if !self.encrypted {
            return Ok(());
        }
        let man = read_plist(&self.dir.join("Manifest.plist")).ok_or("Manifest.plist mancante")?;
        let d = man.as_dictionary().ok_or("Manifest.plist non valido")?;
        let keybag = d.get("BackupKeyBag").and_then(|v| v.as_data()).ok_or("BackupKeyBag mancante")?;
        let mk = d.get("ManifestKey").and_then(|v| v.as_data()).filter(|k| k.len() > 4).ok_or("ManifestKey mancante")?;
        let keys = crypto::unlock(keybag, password.as_bytes())?;
        let class = u32::from_le_bytes(mk[..4].try_into()?);
        let key = crypto::file_key(&keys, class, &mk[4..]).ok_or("ManifestKey non sbloccabile")?;
        let path = self.dir.join("Manifest.db");
        let mut db = vec![];
        CbcReader::new(&path, &key, 0, std::fs::metadata(&path)?.len())?.read_to_end(&mut db)?;
        let manifest = mem_db(crypto::strip_pkcs7(db))?;
        manifest.query_row("SELECT count(*) FROM Files", [], |r| r.get::<_, i64>(0))?;
        *self.unlocked.lock().unwrap_or_else(|e| e.into_inner()) = Some(Unlocked { keys, manifest });
        Ok(())
    }

    pub fn ddna(&self) -> Option<&datasets::DdnaMeta> {
        self.summary.ddna.as_ref()
    }

    /// Carica il contesto crittografico delle Note protette (verifier account e note cifrate).
    pub fn notes_crypto_context(&self) -> Res<(Option<notes_recovery::AccountCryptoVerifier>, Vec<notes_recovery::ProtectedNoteData>)> {
        self.with(|cx| {
            let db = cx.sqlite("AppDomainGroup-group.com.apple.notes", "NoteStore.sqlite")?;
            Ok(notes_recovery::load_notes_crypto_context(&db))
        })
    }


    /// Sezioni chiave/valore di Info.plist, DDNABackup.plist (iMazing), Manifest.plist, Status.plist.
    pub fn info(&self) -> Vec<(String, Vec<(String, String)>)> {
        let skip = ["Applications", "Installed Applications", "iTunes Files", "iTunes Settings", "BackupKeyBag", "ManifestKey"];
        [("Dispositivo", "Info.plist"), ("Metadati iMazing (DDNABackup.plist)", "DDNABackup.plist"), ("Manifest", "Manifest.plist"), ("Stato backup", "Status.plist")]
            .iter()
            .filter_map(|(title, file)| {
                let v = read_plist(&self.dir.join(file))?;
                let rows = v.as_dictionary()?.iter().filter(|(k, _)| !skip.contains(&k.as_str())).map(|(k, v)| (k.clone(), plist_text(v))).collect();
                Some((title.to_string(), rows))
            })
            .collect()
    }
}

fn summary(dir: &Path) -> Summary {
    let info = read_plist(&dir.join("Info.plist"));
    let ddna = read_plist(&dir.join("DDNABackup.plist"));
    let ddna_meta = ddna.as_ref().and_then(datasets::parse_ddna_backup);
    let get = |v: &Option<plist::Value>, k: &str| v.as_ref().and_then(|v| v.as_dictionary()?.get(k).map(plist_text)).unwrap_or_default();
    let model = match get(&ddna, "deviceModelDetails") {
        m if !m.is_empty() => m,
        _ => get(&info, "Product Name"),
    };
    Summary {
        name: get(&info, "Device Name"),
        model,
        ios: get(&info, "Product Version"),
        date: get(&info, "Last Backup Date"),
        udid: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size: ddna_meta.as_ref().map(|d| d.total_disk_usage).unwrap_or_else(|| get(&ddna, "totalDiskUsage").parse().unwrap_or(0)),
        ddna: ddna_meta,
    }
}


/// Valore plist come testo leggibile (i blob binari diventano la loro dimensione).
pub fn plist_text(v: &plist::Value) -> String {
    use plist::Value::*;
    match v {
        String(s) => s.clone(),
        Boolean(b) => if *b { "sì" } else { "no" }.into(),
        Integer(i) => i.to_string(),
        Real(f) => f.to_string(),
        Date(d) => d.to_xml_format(),
        Data(b) => format!("<{} byte>", b.len()),
        Uid(u) => u.get().to_string(),
        Array(a) => format!("[{}]", a.iter().map(plist_text).collect::<Vec<_>>().join(", ")),
        Dictionary(d) => format!("{{{}}}", d.iter().map(|(k, v)| format!("{k}: {}", plist_text(v))).collect::<Vec<_>>().join(", ")),
        _ => std::string::String::new(),
    }
}

pub(crate) fn read_plist(p: &Path) -> Option<plist::Value> {
    plist::Value::from_file(p).ok()
}

// ---------------------------------------------------------------- SQLite

pub(crate) fn open_db(p: &Path) -> Res<Connection> {
    // La sqlite3.dll di Python non si auto-inizializza (OMIT_AUTOINIT): senza questo rusqlite va in crash.
    unsafe { ffi::sqlite3_initialize() };
    // immutable=1: SQLite non crea -wal/-shm e non tocca il backup.
    let s = p.to_string_lossy().replace('\\', "/").replace('%', "%25").replace('?', "%3f").replace('#', "%23");
    let uri = format!("file:///{}?immutable=1", s.trim_start_matches('/'));
    Ok(Connection::open_with_flags(uri, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?)
}

/// Database SQLite decifrato in memoria (niente file in chiaro su disco).
pub(crate) fn mem_db(mut bytes: Vec<u8>) -> Res<Connection> {
    // header WAL (18-19 = 2) -> rollback journal: sqlite3_deserialize non legge db in modalità WAL
    if bytes.len() > 19 && bytes[18] == 2 {
        bytes[18] = 1;
        bytes[19] = 1;
    }
    unsafe { ffi::sqlite3_initialize() };
    let mut c = Connection::open_in_memory()?;
    let p = NonNull::new(unsafe { ffi::sqlite3_malloc64(bytes.len() as u64) }.cast::<u8>()).ok_or("database vuoto o memoria esaurita")?;
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.as_ptr(), bytes.len()) };
    c.deserialize(DatabaseName::Main, unsafe { OwnedData::from_raw_nonnull(p, bytes.len()) }, true)?;
    Ok(c)
}

pub(crate) fn txt(r: &Row, i: usize) -> String {
    match r.get_ref(i) {
        Ok(ValueRef::Text(t)) | Ok(ValueRef::Blob(t)) => String::from_utf8_lossy(t).into_owned(),
        Ok(ValueRef::Integer(n)) => n.to_string(),
        Ok(ValueRef::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn num(r: &Row, i: usize) -> f64 {
    match r.get_ref(i) {
        Ok(ValueRef::Integer(n)) => n as f64,
        Ok(ValueRef::Real(f)) => f,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------- file del backup

pub fn check_id(id: &str) -> Res<()> {
    if id.len() != 40 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("fileID non valido".into());
    }
    Ok(())
}

pub(crate) fn blob_path(dir: &Path, id: &str) -> Res<PathBuf> {
    check_id(id)?;
    let p = dir.join(&id[..2]).join(id);
    Ok(if p.exists() { p } else { dir.join(id) }) // layout piatto pre-iOS 10
}

/// MBFile (NSKeyedArchiver nella colonna Files.file).
pub(crate) struct MbFile {
    pub size: u64,
    pub mtime: i64,
    /// (classe di protezione, chiave avvolta) — solo nei backup cifrati
    pub key: Option<(u32, Vec<u8>)>,
}

pub(crate) fn mbfile(blob: &[u8]) -> MbFile {
    let obj = plist::Value::from_reader(Cursor::new(blob)).ok();
    let objs = obj.as_ref().and_then(|v| v.as_dictionary()?.get("$objects")?.as_array());
    let d = objs.and_then(|o| o.get(1)?.as_dictionary());
    let int = |k| d.and_then(|d| d.get(k)?.as_signed_integer()).unwrap_or(0);
    // EncryptionKey -> UID -> {"NS.data": classe (u32 LE) + chiave avvolta (40 byte)}
    let wrapped = match d.and_then(|d| d.get("EncryptionKey")) {
        Some(plist::Value::Uid(u)) => objs.and_then(|o| o.get(u.get() as usize)?.as_dictionary()?.get("NS.data")?.as_data()),
        _ => None,
    };
    let key = wrapped.filter(|k| k.len() > 4).map(|k| {
        let class = match int("ProtectionClass") {
            c if c > 0 => c as u32,
            _ => u32::from_le_bytes(k[..4].try_into().unwrap()),
        };
        (class, k[4..].to_vec())
    });
    MbFile { size: int("Size").max(0) as u64, mtime: int("LastModified"), key }
}

/// Contenuto in chiaro di un file del backup (eventualmente decifrato al volo).
pub struct Content {
    path: PathBuf,
    pub size: u64,
    key: Option<[u8; 32]>,
}

impl Content {
    pub fn reader(&self, start: u64, len: u64) -> std::io::Result<Box<dyn Read + Send>> {
        Ok(match &self.key {
            Some(k) => Box::new(CbcReader::new(&self.path, k, start, len)?),
            None => {
                let mut f = File::open(&self.path)?;
                f.seek(SeekFrom::Start(start))?;
                Box::new(f.take(len))
            }
        })
    }

    pub fn read_all(&self) -> Res<Vec<u8>> {
        let mut v = Vec::with_capacity(self.size as usize);
        self.reader(0, self.size)?.read_to_end(&mut v)?;
        Ok(v)
    }

    /// Copia il file (decifrato) in `dest`.
    pub fn save_to(&self, dest: &Path) -> Res<()> {
        std::io::copy(&mut self.reader(0, self.size)?, &mut File::create(dest)?)?;
        Ok(())
    }
}

/// Backup aperto: Manifest.db + chiavi (se cifrato).
pub struct Ctx<'a> {
    pub(crate) dir: &'a Path,
    pub(crate) m: &'a Connection,
    pub(crate) keys: Option<&'a ClassKeys>,
}

impl Ctx<'_> {
    pub fn find(&self, domain: &str, rel: &str) -> Option<String> {
        let mut st = self.m.prepare_cached("SELECT fileID FROM Files WHERE domain=?1 AND relativePath=?2").ok()?;
        st.query_row([domain, rel], |r| r.get(0)).ok()
    }

    pub fn content(&self, id: &str) -> Res<Content> {
        let path = blob_path(self.dir, id)?;
        let Some(keys) = self.keys else {
            return Ok(Content { size: std::fs::metadata(&path)?.len(), path, key: None });
        };
        let blob: Vec<u8> = self.m.query_row("SELECT file FROM Files WHERE fileID=?1", [id], |r| r.get(0))?;
        let mb = mbfile(&blob);
        let (class, wrapped) = mb.key.ok_or("file senza EncryptionKey")?;
        let key = crypto::file_key(keys, class, &wrapped).ok_or("chiave del file non sbloccabile")?;
        Ok(Content { path, size: mb.size, key: Some(key) })
    }

    pub(crate) fn sqlite(&self, domain: &str, rel: &str) -> Res<Connection> {
        let id = self.find(domain, rel).ok_or_else(|| format!("{rel} non presente nel backup"))?;
        match self.keys {
            None => open_db(&blob_path(self.dir, &id)?),
            Some(_) => mem_db(self.content(&id)?.read_all()?),
        }
    }
}

// ---------------------------------------------------------------- utilità di parsing

/// Chiave di confronto: ultime 9 cifre per i numeri, minuscolo per le email.
pub(crate) fn handle_key(s: &str) -> String {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    if s.contains('@') || d.len() < 6 {
        s.trim().to_lowercase()
    } else {
        d[d.len().saturating_sub(9)..].to_string()
    }
}

/// Testo da message.attributedBody (NSArchiver "streamtyped"): NSString ... '+' <len> <utf8>.
pub(crate) fn typedstream_text(b: &[u8]) -> Option<String> {
    let i = b.windows(8).position(|w| w == b"NSString")? + 8;
    let j = i + b.get(i..(i + 12).min(b.len()))?.iter().position(|&c| c == b'+')? + 1;
    let (len, start) = match *b.get(j)? {
        0x81 => (u16::from_le_bytes([*b.get(j + 1)?, *b.get(j + 2)?]) as usize, j + 3),
        0x82 => (u32::from_le_bytes(b.get(j + 1..j + 5)?.try_into().ok()?) as usize, j + 5),
        n => (n as usize, j + 1),
    };
    Some(String::from_utf8_lossy(b.get(start..start + len)?).into_owned())
}

/// Date Apple: nanosecondi da iOS 11 (sms.db), altrimenti secondi; 0 = data assente.
pub(crate) fn apple_ts(v: f64) -> f64 {
    if v == 0.0 {
        return 0.0;
    }
    (if v > 1e11 { v / 1e9 } else { v }) + APPLE_EPOCH
}

fn varint(b: &mut &[u8]) -> Option<u64> {
    let mut r = 0u64;
    for i in 0..10 {
        let c = *b.get(i)?;
        r |= ((c & 0x7f) as u64) << (7 * i);
        if c < 0x80 {
            *b = &b[i + 1..];
            return Some(r);
        }
    }
    None
}

/// Primo campo length-delimited `want` di un messaggio protobuf.
pub(crate) fn pb_field(mut b: &[u8], want: u64) -> Option<&[u8]> {
    while !b.is_empty() {
        let key = varint(&mut b)?;
        match key & 7 {
            0 => {
                varint(&mut b)?;
            }
            1 => b = b.get(8..)?,
            5 => b = b.get(4..)?,
            2 => {
                let n = varint(&mut b)? as usize;
                let v = b.get(..n)?;
                if key >> 3 == want {
                    return Some(v);
                }
                b = &b[n..];
            }
            _ => return None,
        }
    }
    None
}

/// Tipo MIME e se il browser/viewer può mostrarlo direttamente.
pub fn mime(name: &str) -> (&'static str, bool) {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "thumb" => ("image/jpeg", true),
        "png" => ("image/png", true),
        "gif" => ("image/gif", true),
        "webp" => ("image/webp", true),
        "heic" | "heif" => ("image/heic", true),
        // .mov dell'iPhone è un contenitore ISO-BMFF
        "mov" | "mp4" | "m4v" => ("video/mp4", true),
        "m4a" => ("audio/mp4", true),
        "mp3" => ("audio/mpeg", true),
        // note vocali WhatsApp: Opus in contenitore Ogg
        "opus" | "ogg" => ("audio/ogg", true),
        "aac" => ("audio/aac", true),
        "wav" | "caf" | "amr" => ("audio/wav", true),
        "pdf" => ("application/pdf", true),
        "txt" | "log" | "json" | "xml" | "csv" | "vcf" => ("text/plain; charset=utf-8", true),
        _ => ("application/octet-stream", false),
    }
}

pub fn is_video(name: &str) -> bool {
    mime(name).0.starts_with("video/")
}

pub fn is_image(name: &str) -> bool {
    mime(name).0.starts_with("image/")
}

pub fn is_audio(name: &str) -> bool {
    mime(name).0.starts_with("audio/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::tests::{aes_wrap, cbc_encrypt};

    #[test]
    fn parsers() {
        // protobuf: {2: {3: {2: "ciao"}}}
        let pb = [0x12, 0x08, 0x1a, 0x06, 0x12, 0x04, b'c', b'i', b'a', b'o'];
        assert_eq!(pb_field(pb_field(pb_field(&pb, 2).unwrap(), 3).unwrap(), 2).unwrap(), b"ciao");
        // typedstream: lunghezza breve e lunga (0x81 + u16 LE)
        let mut ts = b"\x04\x0bstreamtyped\x81\xe8\x03\x84\x01@\x84\x84\x84\x08NSString\x01\x94\x84\x01+\x05hello".to_vec();
        assert_eq!(typedstream_text(&ts).as_deref(), Some("hello"));
        let long = "x".repeat(300);
        ts.truncate(ts.len() - 6);
        ts.extend([0x81, 0x2c, 0x01]);
        ts.extend(long.as_bytes());
        assert_eq!(typedstream_text(&ts), Some(long));
        assert_eq!(handle_key("+39 345 039 2702"), handle_key("3450392702"));
        assert_eq!(apple_ts(767017549039377024.0) as i64, 1_745_324_749);
        assert_eq!(apple_ts(0.0), 0.0);
        assert!(blob_path(Path::new("x"), "../../etc/passwd").is_err());
        assert!(is_video("A.MOV") && is_image("b.heic") && is_audio("v.opus") && !is_video("x.txt"));
    }

    fn tlv(tag: &str, v: &[u8]) -> Vec<u8> {
        [tag.as_bytes(), &(v.len() as u32).to_be_bytes(), v].concat()
    }

    fn mbfile_blob(size: u64, class: u32, wrapped: &[u8]) -> Vec<u8> {
        use plist::{Dictionary, Uid, Value};
        let mut file = Dictionary::new();
        file.insert("Size".into(), Value::Integer((size as i64).into()));
        file.insert("ProtectionClass".into(), Value::Integer((class as i64).into()));
        file.insert("EncryptionKey".into(), Value::Uid(Uid::new(2)));
        let mut key = Dictionary::new();
        key.insert("NS.data".into(), Value::Data([class.to_le_bytes().to_vec(), wrapped.to_vec()].concat()));
        let mut root = Dictionary::new();
        root.insert("$objects".into(), Value::Array(vec!["$null".into(), Value::Dictionary(file), Value::Dictionary(key)]));
        let mut out = vec![];
        Value::Dictionary(root).to_writer_binary(&mut out).unwrap();
        out
    }

    /// Backup cifrato sintetico (stesso formato di iTunes, iterazioni ridotte): sblocco, Manifest.db, file.
    #[test]
    fn encrypted_backup_roundtrip() {
        let dir = std::env::temp_dir().join(format!("imancraft-enc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (password, dpsl, salt) = (b"segreta".as_slice(), [1u8; 20], [2u8; 20]);

        // chiavi: password -> kek; kek avvolge la chiave di classe 3; classe 3 avvolge manifest e file
        let mut s1 = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password, &dpsl, 3, &mut s1);
        let mut kek = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(&s1, &salt, 2, &mut kek);
        let (class_key, manifest_key, file_key) = ([3u8; 32], [4u8; 32], [5u8; 32]);
        let keybag = [
            tlv("VERS", &4u32.to_be_bytes()),
            tlv("TYPE", &1u32.to_be_bytes()),
            tlv("UUID", &[9; 16]),
            tlv("WRAP", &0u32.to_be_bytes()),
            tlv("SALT", &salt),
            tlv("ITER", &2u32.to_be_bytes()),
            tlv("DPWT", &1u32.to_be_bytes()),
            tlv("DPIC", &3u32.to_be_bytes()),
            tlv("DPSL", &dpsl),
            tlv("UUID", &[8; 16]),
            tlv("CLAS", &3u32.to_be_bytes()),
            tlv("WRAP", &2u32.to_be_bytes()),
            tlv("KTYP", &0u32.to_be_bytes()),
            tlv("WPKY", &aes_wrap(&kek, &class_key)),
        ]
        .concat();

        let content = b"contenuto segreto del file".repeat(1000);
        let id = "ab".repeat(20);
        let db_path = dir.join("plain.db");
        {
            unsafe { ffi::sqlite3_initialize() };
            let c = Connection::open(&db_path).unwrap();
            c.execute_batch("CREATE TABLE Files (fileID TEXT PRIMARY KEY, domain TEXT, relativePath TEXT, flags INTEGER, file BLOB); PRAGMA journal_mode=WAL;").unwrap();
            c.execute(
                "INSERT INTO Files VALUES (?1, 'HomeDomain', 'Library/test.txt', 1, ?2)",
                rusqlite::params![id, mbfile_blob(content.len() as u64, 3, &aes_wrap(&class_key, &file_key))],
            )
            .unwrap();
        }
        let plain_db = std::fs::read(&db_path).unwrap();
        std::fs::write(dir.join("Manifest.db"), cbc_encrypt(&manifest_key, &plain_db)).unwrap();
        std::fs::create_dir_all(dir.join("ab")).unwrap();
        std::fs::write(dir.join("ab").join(&id), cbc_encrypt(&file_key, &content)).unwrap();

        let mut man = plist::Dictionary::new();
        man.insert("IsEncrypted".into(), true.into());
        man.insert("BackupKeyBag".into(), plist::Value::Data(keybag));
        man.insert("ManifestKey".into(), plist::Value::Data([3u32.to_le_bytes().to_vec(), aes_wrap(&class_key, &manifest_key)].concat()));
        plist::Value::Dictionary(man).to_file_xml(dir.join("Manifest.plist")).unwrap();

        let b = Backup::open(dir.clone());
        assert!(b.encrypted && b.is_locked());
        assert_eq!(b.with(|_| Ok(())).unwrap_err().to_string(), LOCKED);
        assert!(b.unlock("sbagliata").is_err());
        b.unlock("segreta").unwrap();
        assert!(!b.is_locked());

        b.with(|cx| {
            assert_eq!(cx.find("HomeDomain", "Library/test.txt").as_deref(), Some(id.as_str()));
            let c = cx.content(&id)?;
            assert_eq!(c.read_all()?, content);
            let mut part = vec![];
            c.reader(1000, 50)?.read_to_end(&mut part)?;
            assert_eq!(part, &content[1000..1050]);
            Ok(())
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
