//! Dati standard di iOS letti dal backup: app, file, contatti, messaggi, note, chiamate, Safari, foto.

use crate::{apple_ts, handle_key, mbfile, num, pb_field, read_plist, txt, typedstream_text, Ctx, Res, APPLE_EPOCH};
use rusqlite::Connection;
use std::collections::HashMap;
use std::io::{Cursor, Read};

// ---------------------------------------------------------------- app

#[derive(Clone, Debug, Default)]
pub struct App {
    pub bundle: String,
    pub name: String,
    pub artist: String,
    pub version: String,
    pub files: i64,
    /// PNG dell'icona (Info.plist → PlaceholderIcon)
    pub icon: Option<Vec<u8>>,
}

pub fn apps(cx: &Ctx) -> Res<Vec<App>> {
    let counts: HashMap<String, i64> = domains(cx)?.into_iter().map(|d| (d.name, d.count)).collect();
    let info = read_plist(&cx.dir.join("Info.plist")).ok_or("Info.plist mancante")?;
    let apps = info.as_dictionary().and_then(|d| d.get("Applications")?.as_dictionary()).ok_or("nessuna app")?;
    let mut out: Vec<App> = apps
        .iter()
        .map(|(bundle, v)| {
            let d = v.as_dictionary();
            let md = d.and_then(|d| d.get("iTunesMetadata")?.as_data()).and_then(|b| plist::Value::from_reader(Cursor::new(b)).ok());
            let g = |k: &str| md.as_ref().and_then(|m| m.as_dictionary()?.get(k)?.as_string().map(str::to_owned)).unwrap_or_default();
            App {
                bundle: bundle.clone(),
                name: Some(g("itemName")).filter(|n| !n.is_empty()).unwrap_or_else(|| bundle.clone()),
                artist: g("artistName"),
                version: g("bundleShortVersionString"),
                files: counts.get(&format!("AppDomain-{bundle}")).copied().unwrap_or(0),
                icon: d.and_then(|d| d.get("PlaceholderIcon")?.as_data()).map(<[u8]>::to_vec),
            }
        })
        .collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

// ---------------------------------------------------------------- file system

#[derive(Clone, Debug)]
pub struct Domain {
    pub name: String,
    pub count: i64,
}

/// Conta file+cartelle: basta l'indice FilesDomainIdx senza leggere le righe (Manifest.db può pesare centinaia di MB).
pub fn domains(cx: &Ctx) -> Res<Vec<Domain>> {
    let mut st = cx.m.prepare("SELECT domain, count(*) FROM Files GROUP BY domain ORDER BY domain")?;
    let rows = st.query_map([], |r| Ok(Domain { name: r.get(0)?, count: r.get(1)? }))?.collect::<Result<_, _>>()?;
    Ok(rows)
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub id: String,
    pub size: u64,
    pub mtime: i64,
}

/// Figli diretti di `path` nel dominio (cartelle prima).
pub fn ls(cx: &Ctx, domain: &str, path: &str) -> Res<Vec<Entry>> {
    let (sql, params): (&str, Vec<String>) = if path.is_empty() {
        ("SELECT relativePath, flags, fileID, file FROM Files WHERE domain=?1 AND relativePath<>'' AND instr(relativePath,'/')=0", vec![domain.into()])
    } else {
        // figli diretti: path/... senza ulteriori '/' ('0' è il carattere dopo '/')
        (
            "SELECT relativePath, flags, fileID, file FROM Files WHERE domain=?1 AND relativePath>?2 AND relativePath<?3 AND instr(substr(relativePath,?4),'/')=0",
            vec![domain.into(), format!("{path}/"), format!("{path}0"), (path.chars().count() + 2).to_string()],
        )
    };
    let mut st = cx.m.prepare(sql)?;
    let mut out: Vec<Entry> = st
        .query_map(rusqlite::params_from_iter(params), |r| {
            let rel = txt(r, 0);
            let mb = r.get::<_, Option<Vec<u8>>>(3)?.map(|b| mbfile(&b));
            Ok(Entry {
                name: rel.rsplit('/').next().unwrap_or(&rel).to_string(),
                dir: r.get::<_, i64>(1)? == 2,
                id: txt(r, 2),
                size: mb.as_ref().map_or(0, |m| m.size),
                mtime: mb.as_ref().map_or(0, |m| m.mtime),
                path: rel,
            })
        })?
        .collect::<Result<_, _>>()?;
    out.sort_by_key(|e| (!e.dir, e.name.to_lowercase()));
    Ok(out)
}

/// Plist del backup in XML leggibile.
pub fn plist_xml(cx: &Ctx, id: &str) -> Res<String> {
    let v = plist::Value::from_reader(Cursor::new(cx.content(id)?.read_all()?))?;
    let mut out = vec![];
    v.to_writer_xml(&mut out)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

// ---------------------------------------------------------------- contatti

pub(crate) const ADDRESS_BOOK: (&str, &str) = ("HomeDomain", "Library/AddressBook/AddressBook.sqlitedb");

#[derive(Clone, Debug, Default)]
pub struct Contact {
    pub name: String,
    pub org: String,
    /// (etichetta, valore)
    pub phones: Vec<(String, String)>,
    pub emails: Vec<(String, String)>,
}

fn clean_label(l: &str) -> String {
    l.trim_start_matches("_$!<").trim_end_matches(">!$_").trim_start_matches("X-").to_string()
}

pub fn contacts(cx: &Ctx) -> Res<Vec<Contact>> {
    let ab = cx.sqlite(ADDRESS_BOOK.0, ADDRESS_BOOK.1)?;
    let mut st = ab.prepare(
        "SELECT p.ROWID, p.First, p.Last, p.Organization, mv.property, mv.value, l.value
         FROM ABPerson p
         LEFT JOIN ABMultiValue mv ON mv.record_id=p.ROWID AND mv.property IN (3,4)
         LEFT JOIN ABMultiValueLabel l ON l.ROWID=mv.label
         ORDER BY p.ROWID",
    )?;
    let mut people: Vec<(i64, Contact)> = vec![];
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        if people.last().map(|p| p.0) != Some(id) {
            let name = format!("{} {}", txt(r, 1), txt(r, 2)).trim().to_string();
            let org = txt(r, 3);
            people.push((id, Contact { name: if name.is_empty() { org.clone() } else { name }, org, ..Default::default() }));
        }
        let entry = (clean_label(&txt(r, 6)), txt(r, 5));
        let c = &mut people.last_mut().unwrap().1;
        match r.get::<_, Option<i64>>(4)? {
            Some(3) => c.phones.push(entry),
            Some(4) => c.emails.push(entry),
            _ => {}
        }
    }
    let mut out: Vec<Contact> = people.into_iter().map(|(_, p)| p).collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// Numero/email (chiave `handle_key`) → nome in rubrica.
pub(crate) fn contact_names(cx: &Ctx) -> HashMap<String, String> {
    let Ok(ab) = cx.sqlite(ADDRESS_BOOK.0, ADDRESS_BOOK.1) else {
        return HashMap::new();
    };
    let q = "SELECT mv.value, trim(coalesce(p.First,'')||' '||coalesce(p.Last,'')), p.Organization
             FROM ABMultiValue mv JOIN ABPerson p ON p.ROWID=mv.record_id WHERE mv.property IN (3,4)";
    let mut map = HashMap::new();
    if let Ok(mut st) = ab.prepare(q) {
        let _ = st
            .query_map([], |r| {
                let name = txt(r, 1);
                map.insert(handle_key(&txt(r, 0)), if name.is_empty() { txt(r, 2) } else { name });
                Ok(())
            })
            .map(|it| it.for_each(drop));
    }
    map
}

// ---------------------------------------------------------------- iMessage / SMS

#[derive(Clone, Debug)]
pub struct Chat {
    pub id: i64,
    pub title: String,
    pub service: String,
    pub count: i64,
    pub last: f64,
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    /// fileID nel backup (None = allegato non incluso)
    pub id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub t: f64,
    pub me: bool,
    pub text: String,
    pub from: String,
    pub att: Vec<Attachment>,
}

const SMS: (&str, &str) = ("HomeDomain", "Library/SMS/sms.db");

pub fn chats(cx: &Ctx) -> Res<Vec<Chat>> {
    let names = contact_names(cx);
    let name_of = |h: &str| names.get(&handle_key(h)).cloned().unwrap_or_else(|| h.to_string());
    let sms = cx.sqlite(SMS.0, SMS.1)?;
    let mut st = sms.prepare(
        "SELECT c.ROWID, c.display_name, c.chat_identifier, c.service_name, count(j.message_id), max(j.message_date),
                (SELECT group_concat(h.id, '|') FROM chat_handle_join ch JOIN handle h ON h.ROWID=ch.handle_id WHERE ch.chat_id=c.ROWID)
         FROM chat c LEFT JOIN chat_message_join j ON j.chat_id=c.ROWID
         GROUP BY c.ROWID ORDER BY max(j.message_date) DESC",
    )?;
    let out = st
        .query_map([], |r| {
            let members = txt(r, 6);
            let title = match txt(r, 1) {
                t if !t.is_empty() => t,
                _ if !members.is_empty() => members.split('|').map(&name_of).collect::<Vec<_>>().join(", "),
                _ => name_of(&txt(r, 2)),
            };
            Ok(Chat { id: r.get(0)?, title, service: txt(r, 3), count: r.get(4)?, last: apple_ts(num(r, 5)) })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

pub fn chat(cx: &Ctx, chat_id: i64) -> Res<Vec<Message>> {
    let names = contact_names(cx);
    let sms = cx.sqlite(SMS.0, SMS.1)?;
    let mut att = sms.prepare(
        "SELECT a.filename, a.mime_type, a.transfer_name FROM message_attachment_join maj
         JOIN attachment a ON a.ROWID=maj.attachment_id WHERE maj.message_id=?1",
    )?;
    let mut st = sms.prepare(
        "SELECT m.ROWID, m.date, m.is_from_me, m.text, m.attributedBody, h.id, m.cache_has_attachments
         FROM chat_message_join j JOIN message m ON m.ROWID=j.message_id LEFT JOIN handle h ON h.ROWID=m.handle_id
         WHERE j.chat_id=?1 ORDER BY m.date, m.ROWID",
    )?;
    let mut out = vec![];
    let mut rows = st.query([chat_id])?;
    while let Some(r) = rows.next()? {
        let mut text = txt(r, 3);
        if text.is_empty() {
            text = r.get::<_, Option<Vec<u8>>>(4)?.and_then(|b| typedstream_text(&b)).unwrap_or_default();
        }
        let from = txt(r, 5);
        let mut atts = vec![];
        if r.get::<_, i64>(6)? != 0 {
            let mut ar = att.query([r.get::<_, i64>(0)?])?;
            while let Some(a) = ar.next()? {
                // "~/Library/SMS/Attachments/..." -> MediaDomain "Library/SMS/Attachments/..."
                let fname = txt(a, 0);
                let rel = fname.trim_start_matches("~/").trim_start_matches("/var/mobile/");
                atts.push(Attachment { name: txt(a, 2), mime: txt(a, 1), id: cx.find("MediaDomain", rel) });
            }
        }
        out.push(Message {
            t: apple_ts(num(r, 1)),
            me: r.get::<_, i64>(2)? != 0,
            text: text.replace('\u{FFFC}', "").trim().to_string(),
            from: names.get(&handle_key(&from)).cloned().unwrap_or(from),
            att: atts,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- note

#[derive(Clone, Debug)]
pub struct Note {
    pub title: String,
    pub folder: String,
    pub modified: f64,
    pub locked: bool,
    pub text: String,
}

/// ZICNOTEDATA.ZDATA = gzip(protobuf): Document.2 -> Note.3 -> note_text.2
fn note_text(zdata: &[u8]) -> Option<String> {
    let mut d = vec![];
    flate2::read::GzDecoder::new(zdata).read_to_end(&mut d).ok()?;
    let t = pb_field(pb_field(pb_field(&d, 2)?, 3)?, 2)?;
    Some(String::from_utf8_lossy(t).into_owned())
}

pub fn notes(cx: &Ctx) -> Res<Vec<Note>> {
    let db = cx.sqlite("AppDomainGroup-group.com.apple.notes", "NoteStore.sqlite")?;
    // ponytail: nomi colonna di iOS 13+ (ZTITLE1/ZMODIFICATIONDATE1/ZTITLE2); versioni più vecchie usano schemi diversi.
    let mut st = db.prepare(
        "SELECT n.ZTITLE1, n.ZSNIPPET, n.ZMODIFICATIONDATE1, d.ZDATA, f.ZTITLE2, n.ZISPASSWORDPROTECTED
         FROM ZICCLOUDSYNCINGOBJECT n JOIN ZICNOTEDATA d ON d.ZNOTE=n.Z_PK
         LEFT JOIN ZICCLOUDSYNCINGOBJECT f ON f.Z_PK=n.ZFOLDER
         WHERE coalesce(n.ZMARKEDFORDELETION,0)=0 ORDER BY n.ZMODIFICATIONDATE1 DESC",
    )?;
    let out = st
        .query_map([], |r| {
            let locked = num(r, 5) != 0.0;
            let body = if locked { None } else { r.get::<_, Option<Vec<u8>>>(3)?.and_then(|z| note_text(&z)) };
            Ok(Note {
                title: txt(r, 0),
                folder: txt(r, 4),
                modified: num(r, 2) + APPLE_EPOCH,
                locked,
                text: body.unwrap_or_else(|| txt(r, 1)).replace('\u{FFFC}', "[allegato]"),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

// ---------------------------------------------------------------- chiamate & Safari (solo backup cifrati)

#[derive(Clone, Debug)]
pub struct Call {
    pub who: String,
    pub address: String,
    pub t: f64,
    pub duration: f64,
    pub out: bool,
    pub answered: bool,
    pub kind: &'static str,
    pub provider: String,
}

pub fn calls(cx: &Ctx) -> Res<Vec<Call>> {
    let names = contact_names(cx);
    let db = cx
        .sqlite("HomeDomain", "Library/CallHistoryDB/CallHistory.storedata")
        .map_err(|e| format!("{e} (il registro chiamate c'è solo nei backup cifrati)"))?;
    let mut st = db.prepare("SELECT ZADDRESS, ZDATE, ZDURATION, ZORIGINATED, ZANSWERED, ZCALLTYPE, ZSERVICE_PROVIDER FROM ZCALLRECORD ORDER BY ZDATE DESC")?;
    let out = st
        .query_map([], |r| {
            let addr = txt(r, 0);
            Ok(Call {
                who: names.get(&handle_key(&addr)).cloned().unwrap_or_else(|| addr.clone()),
                address: addr,
                t: apple_ts(num(r, 1)),
                duration: num(r, 2),
                out: num(r, 3) != 0.0,
                answered: num(r, 4) != 0.0,
                kind: match num(r, 5) as i64 {
                    1 => "Telefono",
                    8 => "FaceTime video",
                    16 => "FaceTime audio",
                    _ => "App",
                },
                provider: txt(r, 6),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct Visit {
    pub t: f64,
    pub url: String,
    pub title: String,
}

pub fn safari(cx: &Ctx) -> Res<Vec<Visit>> {
    let db = cx
        .sqlite("HomeDomain", "Library/Safari/History.db")
        .map_err(|e| format!("{e} (la cronologia Safari c'è solo nei backup cifrati)"))?;
    let mut st = db.prepare(
        "SELECT v.visit_time, i.url, v.title FROM history_visits v JOIN history_items i ON i.id=v.history_item
         ORDER BY v.visit_time DESC LIMIT 10000",
    )?;
    let out = st.query_map([], |r| Ok(Visit { t: apple_ts(num(r, 0)), url: txt(r, 1), title: txt(r, 2) }))?.collect::<Result<_, _>>()?;
    Ok(out)
}

// ---------------------------------------------------------------- foto

#[derive(Clone, Debug, Default)]
pub struct Photo {
    pub pk: i64,
    pub name: String,
    /// originale nel backup (None = solo su iCloud)
    pub id: Option<String>,
    pub size: u64,
    /// miniatura iOS Thumbnails/V2/.../5005.JPG (360x480)
    pub thumb: Option<String>,
    pub cloud: bool,
    pub video: bool,
    pub t: f64,
    pub w: i64,
    pub h: i64,
    pub duration: f64,
    pub fav: bool,
    pub hidden: bool,
    pub trashed: bool,
}

#[derive(Clone, Debug)]
pub struct Album {
    pub title: String,
    pub pks: Vec<i64>,
}

/// Libreria Foto come in iMazing: tutti gli elementi di Photos.sqlite, anche quelli rimasti solo su iCloud
/// (Archiviazione ottimizzata): per quelli nel backup c'è solo la miniatura iOS.
pub fn photos(cx: &Ctx) -> Res<(Vec<Photo>, Vec<Album>)> {
    // file del Rullino presenti nel backup: relativePath -> (fileID, size, mtime)
    let mut files: HashMap<String, (String, u64, i64)> = HashMap::new();
    let mut st = cx.m.prepare(
        "SELECT relativePath, fileID, CASE WHEN relativePath LIKE 'Media/PhotoData/Thumbnails/%' THEN NULL ELSE file END
         FROM Files WHERE domain='CameraRollDomain' AND flags=1 AND (relativePath LIKE 'Media/DCIM/%'
           OR relativePath LIKE 'Media/PhotoData/CPLAssets/%' OR relativePath LIKE 'Media/PhotoData/Thumbnails/V2/%/5005.JPG')",
    )?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let mb = r.get::<_, Option<Vec<u8>>>(2)?.map(|b| mbfile(&b));
        files.insert(txt(r, 0), (txt(r, 1), mb.as_ref().map_or(0, |m| m.size), mb.as_ref().map_or(0, |m| m.mtime)));
    }
    let thumb_of = |dir: &str, name: &str| files.get(&format!("Media/PhotoData/Thumbnails/V2/{dir}/{name}/5005.JPG")).map(|f| f.0.clone());

    let Ok(db) = cx.sqlite("CameraRollDomain", "Media/PhotoData/Photos.sqlite") else {
        // senza Photos.sqlite: solo gli originali presenti nel backup
        let mut out: Vec<Photo> = files
            .iter()
            .filter(|(rel, _)| rel.starts_with("Media/DCIM/"))
            .map(|(rel, (id, size, mtime))| {
                let (dir, name) = rel.trim_start_matches("Media/").rsplit_once('/').unwrap_or(("", rel));
                Photo { name: name.into(), id: Some(id.clone()), thumb: thumb_of(dir, name), size: *size, t: *mtime as f64, video: crate::is_video(name), ..Default::default() }
            })
            .collect();
        out.sort_by(|a, b| b.t.total_cmp(&a.t));
        return Ok((out, vec![]));
    };

    let mut st = db.prepare(
        "SELECT Z_PK, ZDIRECTORY, ZFILENAME, ZKIND, ZDATECREATED, ZWIDTH, ZHEIGHT, ZDURATION, ZFAVORITE, ZHIDDEN, ZTRASHEDSTATE
         FROM ZASSET ORDER BY ZDATECREATED DESC",
    )?;
    let items: Vec<Photo> = st
        .query_map([], |r| {
            let (dir, name) = (txt(r, 1), txt(r, 2));
            let orig = files.get(&format!("Media/{dir}/{name}"));
            Ok(Photo {
                pk: num(r, 0) as i64,
                id: orig.map(|o| o.0.clone()),
                size: orig.map_or(0, |o| o.1),
                thumb: thumb_of(&dir, &name),
                cloud: orig.is_none(),
                video: num(r, 3) as i64 == 1,
                t: apple_ts(num(r, 4)),
                w: num(r, 5) as i64,
                h: num(r, 6) as i64,
                duration: num(r, 7),
                fav: num(r, 8) != 0.0,
                hidden: num(r, 9) != 0.0,
                trashed: num(r, 10) != 0.0,
                name,
            })
        })?
        .filter_map(Result::ok)
        .filter(|p| p.id.is_some() || p.thumb.is_some())
        .collect();
    Ok((items, photo_albums(&db).unwrap_or_default()))
}

/// Album utente (ZGENERICALBUM.ZKIND=2). La tabella di join si chiama Z_<n>ASSETS con colonne Z_<n>ALBUMS / Z_<m>ASSETS,
/// e i numeri cambiano tra versioni di iOS: la cerco nello schema.
fn photo_albums(db: &Connection) -> Res<Vec<Album>> {
    let safe = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_');
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Z\\_%ASSETS' ESCAPE '\\'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for t in tables.iter().filter(|t| safe(t)) {
        let cols: Vec<String> = db.prepare(&format!("PRAGMA table_info({t})"))?.query_map([], |r| r.get(1))?.collect::<Result<_, _>>()?;
        let albums = cols.iter().find(|c| c.ends_with("ALBUMS") && safe(c));
        let assets = cols.iter().find(|c| c.ends_with("ASSETS") && !c.starts_with("Z_FOK") && safe(c));
        let (Some(al), Some(as_)) = (albums, assets) else { continue };
        let mut st = db.prepare(&format!(
            "SELECT a.Z_PK, a.ZTITLE, j.{as_} FROM ZGENERICALBUM a JOIN {t} j ON j.{al}=a.Z_PK
             WHERE a.ZKIND=2 AND coalesce(a.ZTRASHEDSTATE,0)=0 ORDER BY a.ZTITLE"
        ))?;
        let mut map: Vec<(i64, Album)> = vec![];
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let id = num(r, 0) as i64;
            if map.last().map(|a| a.0) != Some(id) {
                map.push((id, Album { title: txt(r, 1), pks: vec![] }));
            }
            map.last_mut().unwrap().1.pks.push(num(r, 2) as i64);
        }
        return Ok(map.into_iter().map(|(_, a)| a).collect());
    }
    Ok(vec![])
}
