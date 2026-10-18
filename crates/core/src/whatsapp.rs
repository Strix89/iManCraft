//! WhatsApp (AppDomainGroup-group.net.whatsapp.WhatsApp.shared): chat, messaggi, media, chiamate, ricerca, export.
//! ChatStorage.sqlite: ZWACHATSESSION (chat), ZWAMESSAGE (messaggi), ZWAMEDIAITEM (media/metadati), ZWAGROUPMEMBER.

use crate::{apple_ts, handle_key, num, pb_field, txt, Ctx, Res, APPLE_EPOCH};
use rusqlite::types::Value as V;
use rusqlite::{Connection, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

pub const DOMAIN: &str = "AppDomainGroup-group.net.whatsapp.WhatsApp.shared";

/// Rubrica JID -> nome, costruita una volta per backup.
type Names = Arc<HashMap<String, String>>;
static NAMES: LazyLock<Mutex<HashMap<PathBuf, Names>>> = LazyLock::new(Default::default);

fn chat_db(cx: &Ctx) -> Res<Connection> {
    cx.sqlite(DOMAIN, "ChatStorage.sqlite").map_err(|e| format!("WhatsApp non presente nel backup ({e})").into())
}

fn names(cx: &Ctx, chat: &Connection) -> Names {
    if let Some(n) = NAMES.lock().unwrap_or_else(|e| e.into_inner()).get(cx.dir) {
        return n.clone();
    }
    // priorità (l'ultimo inserito vince): push name < nome chat < rubrica iOS < rubrica WhatsApp
    let mut map: HashMap<String, String> = HashMap::new();
    fn load(map: &mut HashMap<String, String>, db: &Connection, sql: &str, tilde: bool) {
        if let Ok(mut st) = db.prepare(sql) {
            let _ = st
                .query_map([], |r| {
                    let (jid, name) = (txt(r, 0), txt(r, 1));
                    if !jid.is_empty() && !name.trim().is_empty() {
                        map.insert(jid, if tilde { format!("~{}", name.trim()) } else { name.trim().to_string() });
                    }
                    Ok(())
                })
                .map(|it| it.for_each(drop));
        }
    }
    load(&mut map, chat, "SELECT ZJID, ZPUSHNAME FROM ZWAPROFILEPUSHNAME", true);
    load(&mut map, chat, "SELECT ZCONTACTJID, ZPARTNERNAME FROM ZWACHATSESSION WHERE ZSESSIONTYPE=0", false);
    let ab = crate::data::contact_names(cx);
    let pn: Vec<String> = map.keys().filter(|j| j.ends_with("@s.whatsapp.net")).cloned().collect();
    for jid in pn {
        if let Some(n) = ab.get(&handle_key(jid.split('@').next().unwrap_or(""))) {
            map.insert(jid, n.clone());
        }
    }
    if let Ok(c) = cx.sqlite(DOMAIN, "ContactsV2.sqlite") {
        load(&mut map, &c, "SELECT ZWHATSAPPID, ZFULLNAME FROM ZWAADDRESSBOOKCONTACT", false);
        load(&mut map, &c, "SELECT ZLID, ZFULLNAME FROM ZWAADDRESSBOOKCONTACT", false);
    }
    // LID (identificativi anonimi dei gruppi recenti) -> numero di telefono
    if let Ok(l) = cx.sqlite(DOMAIN, "LID.sqlite") {
        if let Ok(mut st) = l.prepare("SELECT ZLID, ZPHONENUMBER FROM ZWAPHONENUMBERLIDPAIR") {
            let pairs: Vec<(String, String)> = st.query_map([], |r| Ok((txt(r, 0), txt(r, 1)))).map(|it| it.flatten().collect()).unwrap_or_default();
            for (lid, phone) in pairs {
                let pj = if phone.contains('@') { phone } else { format!("{phone}@s.whatsapp.net") };
                let n = map.get(&pj).cloned().unwrap_or_else(|| format!("+{}", pj.split('@').next().unwrap_or("")));
                map.entry(lid).or_insert(n);
            }
        }
    }
    let n = Arc::new(map);
    NAMES.lock().unwrap_or_else(|e| e.into_inner()).insert(cx.dir.to_path_buf(), n.clone());
    n
}

fn name_of(names: &HashMap<String, String>, jid: &str) -> String {
    if let Some(n) = names.get(jid) {
        return n.clone();
    }
    let user = jid.split('@').next().unwrap_or(jid);
    if jid.ends_with("@s.whatsapp.net") {
        format!("+{user}")
    } else {
        user.to_string()
    }
}

/// Categoria di un messaggio (ZMESSAGETYPE raggruppati).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Video,
    Audio,
    Contact,
    Location,
    Event,
    Link,
    Doc,
    System,
    Deleted,
    Sticker,
    Poll,
    Other,
}

/// Tipi ZMESSAGETYPE -> (categoria, etichetta)
fn kind(t: i64) -> (Kind, &'static str) {
    use Kind::*;
    match t {
        0 => (Text, ""),
        1 => (Image, "📷 Foto"),
        2 => (Video, "🎥 Video"),
        3 => (Audio, "🎤 Audio"),
        4 => (Contact, "👤 Contatto"),
        5 => (Location, "📍 Posizione"),
        6 => (Event, "Evento del gruppo"),
        7 => (Link, "🔗 Link"),
        8 => (Doc, "📄 Documento"),
        10 => (System, "Messaggio di sistema"),
        11 => (Video, "GIF"),
        14 => (Deleted, "🚫 Messaggio eliminato"),
        15 => (Sticker, "Sticker"),
        27 => (Link, "Invito a un gruppo"),
        38 => (Image, "📷 Foto (visualizza una volta)"),
        39 => (Video, "🎥 Video (visualizza una volta)"),
        46 => (Poll, "📊 Sondaggio"),
        53 => (Audio, "🎤 Audio (ascolta una volta)"),
        54 => (Video, "⏺ Videomessaggio"),
        _ => (Other, "Messaggio non supportato"),
    }
}

/// Filtro per categoria (lista fissa di tipi: mai input utente nella query).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KindFilter {
    #[default]
    All,
    Text,
    Image,
    Video,
    Audio,
    Doc,
    Sticker,
    Link,
    Location,
    Contact,
    Media,
}

impl KindFilter {
    pub const ALL: [(KindFilter, &'static str); 10] = [
        (KindFilter::All, "Tutti i tipi"),
        (KindFilter::Text, "Testo"),
        (KindFilter::Image, "Foto"),
        (KindFilter::Video, "Video"),
        (KindFilter::Audio, "Audio / vocali"),
        (KindFilter::Doc, "Documenti"),
        (KindFilter::Sticker, "Sticker"),
        (KindFilter::Link, "Link"),
        (KindFilter::Location, "Posizioni"),
        (KindFilter::Contact, "Contatti"),
    ];

    fn sql(self) -> &'static str {
        match self {
            KindFilter::All => "",
            KindFilter::Text => " AND m.ZMESSAGETYPE IN (0,7)",
            KindFilter::Image => " AND m.ZMESSAGETYPE IN (1,38)",
            KindFilter::Video => " AND m.ZMESSAGETYPE IN (2,11,39,54)",
            KindFilter::Audio => " AND m.ZMESSAGETYPE IN (3,53)",
            KindFilter::Doc => " AND m.ZMESSAGETYPE = 8",
            KindFilter::Sticker => " AND m.ZMESSAGETYPE = 15",
            KindFilter::Link => " AND m.ZMESSAGETYPE = 7",
            KindFilter::Location => " AND m.ZMESSAGETYPE = 5",
            KindFilter::Contact => " AND m.ZMESSAGETYPE = 4",
            KindFilter::Media => " AND m.ZMESSAGETYPE IN (1,2,11,38,39,54)",
        }
    }
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ---------------------------------------------------------------- chat

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatKind {
    Chat,
    Group,
    Broadcast,
    Status,
    Community,
}

#[derive(Clone, Debug)]
pub struct WaChat {
    pub id: i64,
    pub jid: String,
    pub title: String,
    pub kind: ChatKind,
    pub count: i64,
    pub last: f64,
    pub last_text: String,
    pub archived: bool,
    /// fileID della foto profilo (.thumb JPEG)
    pub avatar: Option<String>,
}

pub fn chats(cx: &Ctx) -> Res<Vec<WaChat>> {
    let db = chat_db(cx)?;
    // foto profilo: Media/Profile/<utente>-<timestamp>.thumb (tengo la più recente)
    let mut avatars: HashMap<String, (String, String)> = HashMap::new();
    let mut st = cx.m.prepare("SELECT relativePath, fileID FROM Files WHERE domain=?1 AND relativePath LIKE 'Media/Profile/%.thumb'")?;
    let mut rows = st.query([DOMAIN])?;
    while let Some(r) = rows.next()? {
        let rel = txt(r, 0);
        let stem = rel.trim_start_matches("Media/Profile/").trim_end_matches(".thumb");
        if let Some((user, ts)) = stem.rsplit_once('-') {
            let e = avatars.entry(user.to_string()).or_insert((String::new(), String::new()));
            if ts > e.0.as_str() {
                *e = (ts.to_string(), txt(r, 1));
            }
        }
    }
    let mut st = db.prepare(
        // (ZLASTMESSAGETEXT è un protobuf in base64: l'anteprima la prendo dall'ultimo messaggio)
        "SELECT s.Z_PK, s.ZCONTACTJID, s.ZPARTNERNAME, s.ZSESSIONTYPE, s.ZMESSAGECOUNTER, s.ZLASTMESSAGEDATE, lm.ZTEXT, s.ZARCHIVED,
                lm.ZMESSAGETYPE, lm.ZISFROMME
         FROM ZWACHATSESSION s LEFT JOIN ZWAMESSAGE lm ON lm.Z_PK = s.ZLASTMESSAGE
         WHERE coalesce(s.ZREMOVED,0)=0 ORDER BY s.ZLASTMESSAGEDATE DESC",
    )?;
    let out = st
        .query_map([], |r| {
            let jid = txt(r, 1);
            let title = match txt(r, 2) {
                t if !t.trim().is_empty() => t,
                _ => jid.split('@').next().unwrap_or("").to_string(),
            };
            let last = match txt(r, 6) {
                t if !t.is_empty() => t,
                _ => kind(num(r, 8) as i64).1.to_string(),
            };
            Ok(WaChat {
                id: r.get(0)?,
                kind: match num(r, 3) as i64 {
                    1 => ChatKind::Group,
                    2 => ChatKind::Broadcast,
                    3 => ChatKind::Status,
                    4 => ChatKind::Community,
                    _ => ChatKind::Chat,
                },
                count: num(r, 4) as i64,
                last: if num(r, 5) > 0.0 { num(r, 5) + APPLE_EPOCH } else { 0.0 },
                last_text: if num(r, 9) != 0.0 { format!("Tu: {last}") } else { last },
                archived: num(r, 7) != 0.0,
                avatar: avatars.get(jid.split('@').next().unwrap_or("")).map(|a| a.1.clone()),
                title,
                jid,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

// ---------------------------------------------------------------- messaggi

/// Filtri comuni: periodo (secondi Unix), testo, categoria, importanti.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub q: String,
    pub kind: KindFilter,
    pub starred: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none() && self.q.trim().is_empty() && self.kind == KindFilter::All && !self.starred
    }

    fn sql(&self) -> (String, Vec<V>) {
        let mut s = String::from(self.kind.sql());
        let mut p = vec![];
        if let Some(f) = self.from {
            s += " AND m.ZMESSAGEDATE >= ?";
            p.push(V::Real(f - APPLE_EPOCH));
        }
        if let Some(t) = self.to {
            s += " AND m.ZMESSAGEDATE < ?";
            p.push(V::Real(t - APPLE_EPOCH));
        }
        if !self.q.trim().is_empty() {
            s += " AND (m.ZTEXT LIKE ? OR mi.ZTITLE LIKE ? OR mi.ZVCARDNAME LIKE ?)";
            let like = format!("%{}%", self.q.trim());
            p.extend([V::Text(like.clone()), V::Text(like.clone()), V::Text(like)]);
        }
        if self.starred {
            s += " AND m.ZSTARRED = 1";
        }
        (s, p)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WaMedia {
    /// fileID nel backup (None = originale non incluso)
    pub id: Option<String>,
    pub name: String,
    pub size: f64,
    pub duration: f64,
    /// miniatura .thumb (JPEG)
    pub thumb: Option<String>,
    /// larghezza video/immagine in pixel (0 = non determinata)
    pub w: u32,
    /// altezza video/immagine in pixel (0 = non determinata)
    pub h: u32,
}

#[derive(Clone, Debug)]
pub struct WaQuote {
    pub from: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct WaMessage {
    pub pk: i64,
    pub sort: i64,
    pub t: f64,
    pub me: bool,
    pub mtype: i64,
    pub kind: Kind,
    pub label: &'static str,
    pub text: String,
    pub starred: bool,
    pub from: String,
    pub from_jid: String,
    pub chat: i64,
    /// solo nei risultati di ricerca
    pub chat_title: String,
    pub media: Option<WaMedia>,
    pub title: String,
    pub lat: f64,
    pub lon: f64,
    pub vcard_name: String,
    pub vcard: String,
    pub quote: Option<WaQuote>,
}

#[derive(Clone, Debug, Default)]
pub struct WaPage {
    pub messages: Vec<WaMessage>,
    pub older: bool,
    pub newer: bool,
}

const MSG_COLS: &str = "m.Z_PK, m.ZSORT, m.ZMESSAGEDATE, m.ZISFROMME, m.ZMESSAGETYPE, m.ZTEXT, m.ZSTARRED, m.ZGROUPEVENTTYPE,
    m.ZFROMJID, m.ZPUSHNAME, gm.ZMEMBERJID, mi.ZMEDIALOCALPATH, mi.ZTITLE, mi.ZFILESIZE, mi.ZMOVIEDURATION,
    mi.ZLATITUDE, mi.ZLONGITUDE, mi.ZVCARDNAME, mi.ZVCARDSTRING, mi.ZMETADATA, m.ZCHATSESSION,
    mi.ZXMPPTHUMBPATH, mi.ZTHUMBNAILLOCALPATH";
const MSG_FROM: &str = "FROM ZWAMESSAGE m LEFT JOIN ZWAGROUPMEMBER gm ON gm.Z_PK=m.ZGROUPMEMBER LEFT JOIN ZWAMEDIAITEM mi ON mi.Z_PK=m.ZMEDIAITEM";

struct Render<'a> {
    cx: &'a Ctx<'a>,
    /// risultati di ricerca: niente lookup di file e citazioni (migliaia di righe)
    light: bool,
    db: &'a Connection,
    names: &'a HashMap<String, String>,
}

impl Render<'_> {
    fn sender(&self, r: &Row) -> (String, String) {
        if num(r, 3) != 0.0 {
            return ("Tu".into(), String::new());
        }
        // nei gruppi il mittente è il membro; nelle chat singole è il JID della chat.
        // (m.ZPUSHNAME è un blob binario, non testo: il push name viene da ZWAPROFILEPUSHNAME)
        let jid = match txt(r, 10) {
            j if !j.is_empty() => j,
            _ => txt(r, 8),
        };
        if jid.ends_with("@g.us") || jid.ends_with("@broadcast") {
            return (String::new(), String::new()); // evento di sistema senza autore
        }
        (name_of(self.names, &jid), jid)
    }

    fn event(&self, r: &Row, who: &str) -> String {
        let text = txt(r, 5);
        let target = || name_of(self.names, &text);
        let who = if who.is_empty() { "Qualcuno" } else { who };
        match num(r, 7) as i64 {
            1 => format!("{who} ha cambiato il nome del gruppo in «{text}»"),
            2 => format!("{} è entrato/a nel gruppo", target()),
            3 => format!("{who} ha lasciato il gruppo"),
            4 => format!("{who} ha cambiato l'immagine del gruppo"),
            12 => match serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|j| j["subject"].as_str().map(str::to_owned)) {
                Some(s) => format!("Gruppo «{s}» creato"),
                None => "Gruppo creato".into(),
            },
            k if text.contains('@') => format!("Evento del gruppo ({k}): {}", target()),
            k => format!("Evento del gruppo ({k})"),
        }
    }

    /// Messaggio citato: ZMETADATA campo 5 = stanza id, campo 6 = JID dell'autore.
    fn quote(&self, meta: &[u8]) -> Option<WaQuote> {
        let stanza = std::str::from_utf8(pb_field(meta, 5)?).ok()?;
        let author = pb_field(meta, 6).and_then(|a| std::str::from_utf8(a).ok()).unwrap_or("");
        let (text, t, me): (String, i64, bool) = self
            .db
            .prepare_cached("SELECT ZTEXT, ZMESSAGETYPE, ZISFROMME FROM ZWAMESSAGE WHERE ZSTANZAID=?1 LIMIT 1")
            .ok()?
            .query_row([stanza], |r| Ok((txt(r, 0), num(r, 1) as i64, num(r, 2) != 0.0)))
            .unwrap_or_default();
        let text = if text.is_empty() { kind(t).1.to_string() } else { text };
        let from = if me { "Tu".to_string() } else if author.is_empty() { String::new() } else { name_of(self.names, author) };
        Some(WaQuote { from, text: text.chars().take(200).collect() })
    }

    fn media(&self, path: &str, xmpp_thumb: &str, thumb_path: &str, size: f64, duration: f64) -> Option<WaMedia> {
        if path.is_empty() && xmpp_thumb.is_empty() && thumb_path.is_empty() {
            return None;
        }

        let find_file = |p: &str| -> Option<String> {
            if p.is_empty() {
                return None;
            }
            let p = p.replace('\\', "/");
            let p = p.trim_start_matches('/');
            let rel = if p.starts_with("Message/") { p.to_string() } else { format!("Message/{p}") };
            self.cx.find(DOMAIN, &rel).or_else(|| self.cx.find("AppDomain-net.whatsapp.WhatsApp", &rel))
        };

        let id = find_file(path);

        let mut thumb = find_file(thumb_path);
        if thumb.is_none() {
            thumb = find_file(xmpp_thumb);
        }
        if thumb.is_none() && !path.is_empty() {
            let p = path.replace('\\', "/");
            let p = p.trim_start_matches('/');
            let rel = if p.starts_with("Message/") { p.to_string() } else { format!("Message/{p}") };
            thumb = rel.rsplit_once('.').map(|(s, _)| format!("{s}.thumb")).and_then(|t| {
                self.cx.find(DOMAIN, &t).or_else(|| self.cx.find("AppDomain-net.whatsapp.WhatsApp", &t))
            });
        }

        if id.is_none() && thumb.is_none() {
            return None;
        }

        let name = if !path.is_empty() {
            file_name(path)
        } else if !thumb_path.is_empty() {
            file_name(thumb_path)
        } else {
            file_name(xmpp_thumb)
        };

        let (mut w, mut h) = (0u32, 0u32);
        if let Some(fid) = &id {
            let n_lower = name.to_ascii_lowercase();
            if crate::is_video(&n_lower) || n_lower.ends_with(".mp4") || n_lower.ends_with(".mov") || n_lower.ends_with(".3gp") || n_lower.ends_with(".m4v") {
                if self.cx.keys.is_none() {
                    if let Ok(p) = crate::blob_path(self.cx.dir, fid) {
                        if let Ok(mut f) = std::fs::File::open(&p) {
                            if let Some((vw, vh)) = crate::media::mp4_dimensions(&mut f) {
                                w = vw;
                                h = vh;
                            }
                        }
                    }
                } else if let Ok(content) = self.cx.content(fid) {
                    let read_len = content.size.min(262_144);
                    if let Ok(mut r) = content.reader(0, read_len) {
                        let mut buf = vec![0u8; read_len as usize];
                        if std::io::Read::read_exact(&mut r, &mut buf).is_ok() {
                            let mut cursor = std::io::Cursor::new(buf);
                            if let Some((vw, vh)) = crate::media::mp4_dimensions(&mut cursor) {
                                w = vw;
                                h = vh;
                            }
                        }
                    }
                }
            }
        }

        Some(WaMedia { id, name, size, duration, thumb, w, h })
    }

    fn message(&self, r: &Row) -> WaMessage {
        let t = num(r, 4) as i64;
        let (cat, label) = kind(t);
        let (from, from_jid) = self.sender(r);
        let meta: Option<Vec<u8>> = r.get(19).ok().flatten();
        let has_media = !self.light && !matches!(cat, Kind::Text | Kind::Event | Kind::System | Kind::Deleted);
        let path = txt(r, 11);
        let xmpp_thumb = txt(r, 21);
        let thumb_path = txt(r, 22);
        WaMessage {
            pk: num(r, 0) as i64,
            sort: num(r, 1) as i64,
            t: apple_ts(num(r, 2)),
            me: num(r, 3) != 0.0,
            mtype: t,
            kind: cat,
            label,
            text: if cat == Kind::Event { self.event(r, &from) } else { txt(r, 5) },
            starred: num(r, 6) != 0.0,
            chat: num(r, 20) as i64,
            chat_title: String::new(),
            media: if has_media { self.media(&path, &xmpp_thumb, &thumb_path, num(r, 13), num(r, 14)) } else { None },
            title: if has_media { txt(r, 12) } else { String::new() },
            lat: if cat == Kind::Location { num(r, 15) } else { 0.0 },
            lon: if cat == Kind::Location { num(r, 16) } else { 0.0 },
            vcard_name: if cat == Kind::Contact { txt(r, 17).replace("_$!<Name-Separator>!$_", " ") } else { String::new() },
            vcard: if cat == Kind::Contact { txt(r, 18) } else { String::new() },
            quote: meta.as_deref().filter(|_| !self.light).and_then(|b| self.quote(b)),
            from,
            from_jid,
        }
    }
}

fn query_msgs(rd: &Render, where_: &str, params: Vec<V>, order_limit: &str) -> Res<Vec<WaMessage>> {
    let sql = format!("SELECT {MSG_COLS} {MSG_FROM} WHERE 1=1{where_} {order_limit}");
    let mut st = rd.db.prepare(&sql)?;
    let out = st.query_map(rusqlite::params_from_iter(params), |r| Ok(rd.message(r)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Cursore di paginazione dei messaggi di una chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// ultimi messaggi (o dall'inizio del periodo se il filtro ha `from`)
    Latest,
    /// più vecchi di questo ZSORT
    Before(i64),
    /// più nuovi di questo ZSORT
    After(i64),
    /// contesto intorno a questo messaggio (Z_PK)
    Around(i64),
}

/// Pagina di messaggi di una chat (ordine cronologico).
pub fn messages(cx: &Ctx, chat: i64, f: &Filter, cursor: Cursor, limit: i64) -> Res<WaPage> {
    let db = chat_db(cx)?;
    let names = names(cx, &db);
    let rd = Render { cx, db: &db, names: &names, light: false };
    let (fs, fp) = f.sql();
    let base = |extra: &str, v: Option<i64>| {
        let mut p = vec![V::Integer(chat)];
        p.extend(fp.clone());
        if let Some(v) = v {
            p.push(V::Integer(v));
        }
        (format!(" AND m.ZCHATSESSION = ?{fs}{extra}"), p)
    };
    // i ? sono posizionali: l'ordine dei parametri segue l'ordine nel testo (chat, filtri, cursore)
    let older = |s: Option<i64>, n: i64| -> Res<Vec<WaMessage>> {
        let (w, p) = base(if s.is_some() { " AND m.ZSORT < ?" } else { "" }, s);
        let mut v = query_msgs(&rd, &w, p, &format!("ORDER BY m.ZSORT DESC LIMIT {n}"))?;
        v.reverse();
        Ok(v)
    };
    let newer = |s: i64, n: i64, incl: bool| -> Res<Vec<WaMessage>> {
        let (w, p) = base(if incl { " AND m.ZSORT >= ?" } else { " AND m.ZSORT > ?" }, Some(s));
        query_msgs(&rd, &w, p, &format!("ORDER BY m.ZSORT ASC LIMIT {n}"))
    };
    let list = match cursor {
        Cursor::Around(pk) => {
            let s: i64 = db.query_row("SELECT ZSORT FROM ZWAMESSAGE WHERE Z_PK=?1", [pk], |r| r.get(0))?;
            let mut v = older(Some(s), limit / 2)?;
            v.extend(newer(s, limit / 2, true)?);
            v
        }
        Cursor::After(s) => newer(s, limit, false)?,
        Cursor::Before(s) => older(Some(s), limit)?,
        Cursor::Latest if f.from.is_none() => older(None, limit)?,
        Cursor::Latest => newer(i64::MIN, limit, false)?,
    };
    let exists = |cond: &str, s: i64| -> bool {
        let (w, p) = base(cond, Some(s));
        let sql = format!("SELECT 1 {MSG_FROM} WHERE 1=1{w} LIMIT 1");
        db.query_row(&sql, rusqlite::params_from_iter(p), |_| Ok(())).is_ok()
    };
    let first = list.first().map(|m| m.sort);
    let last = list.last().map(|m| m.sort);
    Ok(WaPage {
        older: first.is_some_and(|s| exists(" AND m.ZSORT < ?", s)),
        newer: last.is_some_and(|s| exists(" AND m.ZSORT > ?", s)),
        messages: list,
    })
}

/// Ricerca in tutte le chat (o in una) con periodo, testo, tipo, importanti. Più recenti prima (max 1000).
/// Restituisce (risultati, troncato).
pub fn search(cx: &Ctx, chat: Option<i64>, f: &Filter) -> Res<(Vec<WaMessage>, bool)> {
    let db = chat_db(cx)?;
    let names = names(cx, &db);
    let rd = Render { cx, db: &db, names: &names, light: true };
    let (mut w, mut p) = f.sql();
    if let Some(c) = chat {
        w = format!(" AND m.ZCHATSESSION = ?{w}");
        p.insert(0, V::Integer(c));
    }
    let mut hits = query_msgs(&rd, &w, p, "ORDER BY m.ZMESSAGEDATE DESC LIMIT 1001")?;
    let titles: HashMap<i64, String> = db
        .prepare("SELECT Z_PK, coalesce(nullif(ZPARTNERNAME,''), ZCONTACTJID) FROM ZWACHATSESSION")?
        .query_map([], |r| Ok((r.get(0)?, txt(r, 1))))?
        .collect::<Result<_, _>>()?;
    let truncated = hits.len() > 1000;
    hits.truncate(1000);
    for m in &mut hits {
        m.chat_title = titles.get(&m.chat).cloned().unwrap_or_default();
    }
    Ok((hits, truncated))
}

/// Galleria della chat: foto, video, GIF, sticker, documenti, audio, link (più recenti prima).
pub fn media(cx: &Ctx, chat: i64) -> Res<Vec<WaMessage>> {
    let db = chat_db(cx)?;
    let names = names(cx, &db);
    let rd = Render { cx, db: &db, names: &names, light: false };
    query_msgs(&rd, " AND m.ZCHATSESSION = ? AND m.ZMESSAGETYPE IN (1,2,3,7,8,11,15,38,39,53,54)", vec![V::Integer(chat)], "ORDER BY m.ZSORT DESC LIMIT 5000")
}

#[derive(Clone, Debug)]
pub struct WaCall {
    pub t: f64,
    pub duration: f64,
    pub incoming: bool,
    pub missed: bool,
    pub video: bool,
    pub group: Option<String>,
    pub who: String,
}

pub fn calls(cx: &Ctx) -> Res<Vec<WaCall>> {
    let chat = chat_db(cx)?;
    let names = names(cx, &chat);
    let db = cx.sqlite(DOMAIN, "CallHistory.sqlite")?;
    let mut st = db.prepare(
        "SELECT e.ZDATE, e.ZDURATION, e.ZOUTCOME, a.ZINCOMING, a.ZMISSED, a.ZVIDEO, e.ZGROUPJIDSTRING,
                (SELECT group_concat(p.ZJIDSTRING, '|') FROM ZWACDCALLEVENTPARTICIPANT p WHERE p.Z1PARTICIPANTS = a.Z_PK)
         FROM ZWACDCALLEVENT e LEFT JOIN ZWAAGGREGATECALLEVENT a ON a.Z_PK = e.Z1CALLEVENTS ORDER BY e.ZDATE DESC",
    )?;
    let out = st
        .query_map([], |r| {
            let who: Vec<String> = txt(r, 7).split('|').filter(|j| !j.is_empty()).map(|j| name_of(&names, j)).collect();
            let group = txt(r, 6);
            Ok(WaCall {
                t: apple_ts(num(r, 0)),
                duration: num(r, 1),
                incoming: num(r, 3) != 0.0,
                missed: num(r, 4) != 0.0,
                video: num(r, 5) != 0.0,
                group: if group.is_empty() { None } else { Some(name_of(&names, &group)) },
                who: who.join(", "),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Data locale gg/mm/aaaa hh:mm:ss; `tz` = minuti da sottrarre a UTC (convenzione JS getTimezoneOffset: UTC − locale).
pub fn fmt_date(unix: f64, tz: i64) -> String {
    let s = unix as i64 - tz * 60;
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    // civil_from_days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d:02}/{m:02}/{y} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Csv,
}

/// Esporta la chat (con i filtri) in TXT stile WhatsApp o CSV per Excel. Restituisce (nome file suggerito, contenuto).
pub fn export(cx: &Ctx, chat: i64, f: &Filter, format: ExportFormat, tz: i64) -> Res<(String, String)> {
    let db = chat_db(cx)?;
    let names = names(cx, &db);
    let rd = Render { cx, db: &db, names: &names, light: false };
    let title: String = db.query_row("SELECT coalesce(nullif(ZPARTNERNAME,''), ZCONTACTJID) FROM ZWACHATSESSION WHERE Z_PK=?1", [chat], |r| Ok(txt(r, 0)))?;
    let (fs, mut fp) = f.sql();
    fp.insert(0, V::Integer(chat));
    let msgs = query_msgs(&rd, &format!(" AND m.ZCHATSESSION = ?{fs}"), fp, "ORDER BY m.ZSORT ASC")?;
    let att = |m: &WaMessage| m.media.as_ref().map(|x| x.name.clone()).unwrap_or_default();
    let body_of = |m: &WaMessage| {
        let mut b = m.text.clone();
        if !matches!(m.kind, Kind::Text | Kind::Event) && !m.label.is_empty() {
            let a = att(m);
            let a = if a.is_empty() { a } else { format!(" <{a}>") };
            b = format!("{}{a}{}{b}", m.label, if b.is_empty() { "" } else { " " });
        }
        b
    };
    let (data, ext) = match format {
        ExportFormat::Csv => {
            let q = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
            let mut out = String::from("\u{FEFF}Data;Mittente;Tipo;Testo;Allegato\r\n");
            for m in &msgs {
                let line = [fmt_date(m.t, tz), m.from.clone(), format!("{:?}", m.kind), m.text.clone(), att(m)];
                out += &line.iter().map(|v| q(v)).collect::<Vec<_>>().join(";");
                out += "\r\n";
            }
            (out, "csv")
        }
        ExportFormat::Txt => {
            let mut out = format!("Chat WhatsApp: {title}\r\n\r\n");
            for m in &msgs {
                let who = if m.from.is_empty() { String::new() } else { format!("{}: ", m.from) };
                out += &format!("[{}] {who}{}\r\n", fmt_date(m.t, tz), body_of(m));
            }
            (out, "txt")
        }
    };
    let fname: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    Ok((format!("WhatsApp - {}.{ext}", fname.trim()), data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(fmt_date(0.0, 0), "01/01/1970 00:00:00");
        assert_eq!(fmt_date(1_745_324_749.0, 0), "22/04/2025 12:25:49");
        assert_eq!(fmt_date(1_745_324_749.0, -120), "22/04/2025 14:25:49"); // CEST
        assert_eq!(fmt_date(951_782_400.0, 0), "29/02/2000 00:00:00");
    }

    #[test]
    fn filter_sql_params_follow_placeholders() {
        let f = Filter { from: Some(APPLE_EPOCH + 10.0), to: Some(APPLE_EPOCH + 20.0), q: "x".into(), kind: KindFilter::Doc, starred: true };
        let (s, p) = f.sql();
        assert_eq!(s.matches('?').count(), p.len());
        assert!(s.starts_with(" AND m.ZMESSAGETYPE = 8") && s.ends_with("ZSTARRED = 1"));
        assert!(Filter::default().is_empty() && !f.is_empty());
    }
}
