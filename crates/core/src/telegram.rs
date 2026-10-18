//! Modulo Telegram per iOS (AppDomain-ph.telegra.Telegraph / AppDomainGroup-group.ph.telegra.Telegraph)
//!
//! Ispezione forense dell'applicazione Telegram su iOS:
//! - Rilevamento presenza dell'applicazione e delle estensioni (Share, Widget, NotificationService)
//! - Lettura dei metadati di configurazione e preferenze (`ph.telegra.Telegraph.plist`)
//! - Riconoscimento del database Postbox moderno (`account-*/postbox/db/db_sqlite`) e legacy (`tgdata.sqlite`)
//! - Gestione della policy Apple/Telegram `NSURLIsExcludedFromBackupKey` (esclusione nativa dei messaggi cloud).

use crate::{mem_db, txt, Ctx, Res};
use rusqlite::Connection;

pub const APP_DOMAIN: &str = "AppDomain-ph.telegra.Telegraph";
pub const GROUP_DOMAIN: &str = "AppDomainGroup-group.ph.telegra.Telegraph";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TgChatKind {
    Private,
    Group,
    Supergroup,
    Channel,
}

#[derive(Clone, Debug)]
pub struct TgChat {
    pub id: i64,
    pub title: String,
    pub username: Option<String>,
    pub kind: TgChatKind,
    pub message_count: usize,
    pub last_date: Option<f64>,
    pub last_text: String,
}

#[derive(Clone, Debug)]
pub struct TgMessage {
    pub id: i64,
    pub chat_id: i64,
    pub from_name: String,
    pub text: String,
    pub date: f64,
    pub out: bool,
    pub media_desc: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TelegramMeta {
    pub is_installed: bool,
    pub app_files_count: usize,
    pub has_group_container: bool,
    pub startup_time: Option<i64>,
    pub cache_index_timestamp: Option<i64>,
    pub media_grouping_disabled: bool,
    pub preferred_video_preset: Option<i64>,
    pub databases_found: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum TelegramStatus {
    NotInstalled,
    InstalledExcludedFromBackup(TelegramMeta),
    Available {
        meta: TelegramMeta,
        chats: Vec<TgChat>,
    },
}

/// Rileva lo stato di Telegram all'interno del backup.
pub fn status(cx: &Ctx) -> Res<TelegramStatus> {
    let meta = read_meta(cx)?;
    if !meta.is_installed {
        return Ok(TelegramStatus::NotInstalled);
    }

    // Cerca database noti di Telegram (moderno postbox o legacy tgdata)
    let chats = find_and_extract_chats(cx)?;
    if chats.is_empty() {
        Ok(TelegramStatus::InstalledExcludedFromBackup(meta))
    } else {
        Ok(TelegramStatus::Available { meta, chats })
    }
}

/// Legge i metadati e la configurazione dell'app da Manifest.db e dai file di preferenze.
pub fn read_meta(cx: &Ctx) -> Res<TelegramMeta> {
    let mut meta = TelegramMeta::default();

    // 1. Conta file appartenenti a Telegram
    let mut st = cx.m.prepare(
        "SELECT domain, relativePath FROM Files
         WHERE domain LIKE '%telegra%' AND flags = 1",
    )?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let dom = txt(r, 0);
        let rel = txt(r, 1);
        meta.is_installed = true;
        meta.app_files_count += 1;

        if dom == GROUP_DOMAIN {
            meta.has_group_container = true;
        }

        if rel.contains(".sqlite") || rel.contains("data.mdb") || rel.contains("postbox") {
            meta.databases_found.push(format!("{dom} -> {rel}"));
        }
    }

    if !meta.is_installed {
        return Ok(meta);
    }

    // 2. Leggi le preferenze di Telegram se presenti
    if let Some(id) = cx.find(APP_DOMAIN, "Library/Preferences/ph.telegra.Telegraph.plist") {
        if let Ok(content) = cx.content(&id) {
            if let Ok(data) = content.read_all() {
                if let Ok(val) = plist::Value::from_reader(std::io::Cursor::new(data)) {
                    if let Some(dict) = val.as_dictionary() {
                        meta.startup_time = dict
                            .get("CKStartupTime")
                            .and_then(|v| v.as_signed_integer().or_else(|| v.as_real().map(|f| f as i64)))
                            .map(normalize_apple_or_unix_ts);
                        meta.cache_index_timestamp = dict
                            .get("TelegramCacheIndexTimestamp")
                            .and_then(|v| v.as_signed_integer().or_else(|| v.as_real().map(|f| f as i64)))
                            .map(normalize_apple_or_unix_ts);
                        meta.media_grouping_disabled = dict.get("TG_mediaGroupingDisabled_v0").and_then(|v| v.as_boolean()).unwrap_or(false);
                        meta.preferred_video_preset = dict.get("TG_preferredVideoPreset_v0").and_then(|v| v.as_signed_integer());
                    }
                }
            }
        }
    }

    Ok(meta)
}

fn normalize_apple_or_unix_ts(t: i64) -> i64 {
    if t > 10_000_000_000 {
        t / 1000
    } else if t > 0 && t < 900_000_000 {
        // Differenza epoche: 2001-01-01 (Apple CoreFoundation) -> 1970-01-01 (Unix) = 978307200 s
        t + 978_307_200
    } else {
        t
    }
}

/// Cerca database di chat (Postbox o legacy) ed estrae le conversazioni.
fn find_and_extract_chats(cx: &Ctx) -> Res<Vec<TgChat>> {
    let mut chats = Vec::new();

    // 1. Schema Postbox moderno: cerca db_sqlite in AppDomainGroup o AppDomain
    let mut st = cx.m.prepare(
        "SELECT fileID, relativePath FROM Files
         WHERE (domain = ?1 OR domain = ?2) AND relativePath LIKE '%postbox/db/db_sqlite'",
    )?;
    let matching_files = st
        .query_map([GROUP_DOMAIN, APP_DOMAIN], |r| Ok((txt(r, 0), txt(r, 1))))?
        .collect::<Result<Vec<_>, _>>()?;

    for (file_id, _rel_path) in matching_files {
        if let Ok(conn) = open_temp_sqlite(cx, &file_id) {
            if let Ok(extracted) = extract_postbox_chats(&conn) {
                chats.extend(extracted);
            }
        }
    }

    // 2. Schema Legacy tgdata.sqlite
    let mut st_leg = cx.m.prepare(
        "SELECT fileID FROM Files
         WHERE (domain = ?1 OR domain = ?2) AND relativePath LIKE '%tgdata.sqlite%'",
    )?;
    let legacy_files = st_leg
        .query_map([APP_DOMAIN, GROUP_DOMAIN], |r| Ok(txt(r, 0)))?
        .collect::<Result<Vec<_>, _>>()?;

    for file_id in legacy_files {
        if let Ok(conn) = open_temp_sqlite(cx, &file_id) {
            if let Ok(extracted) = extract_legacy_chats(&conn) {
                chats.extend(extracted);
            }
        }
    }

    Ok(chats)
}

/// Estrae le chat dallo schema Postbox moderno di Telegram iOS.
fn extract_postbox_chats(conn: &Connection) -> Res<Vec<TgChat>> {
    let mut chats = Vec::new();

    let table_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='peers'",
            [],
            |r| Ok(r.get::<_, i64>(0)? > 0),
        )
        .unwrap_or(false);

    if table_exists {
        let mut st = conn.prepare("SELECT id, title, username FROM peers LIMIT 200")?;
        let rows = st.query_map([], |r| {
            Ok(TgChat {
                id: r.get(0)?,
                title: r.get(1).unwrap_or_else(|_| "Chat Telegram".to_string()),
                username: r.get(2).ok(),
                kind: TgChatKind::Private,
                message_count: 0,
                last_date: None,
                last_text: String::new(),
            })
        })?;
        for r in rows.flatten() {
            chats.push(r);
        }
    }

    Ok(chats)
}

/// Estrae le chat dallo schema legacy Telegram iOS (`tgdata.sqlite`).
fn extract_legacy_chats(conn: &Connection) -> Res<Vec<TgChat>> {
    let mut chats = Vec::new();

    let table_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='chats'",
            [],
            |r| Ok(r.get::<_, i64>(0)? > 0),
        )
        .unwrap_or(false);

    if table_exists {
        let mut st = conn.prepare("SELECT id, title FROM chats LIMIT 200")?;
        let rows = st.query_map([], |r| {
            Ok(TgChat {
                id: r.get(0)?,
                title: r.get(1).unwrap_or_else(|_| "Chat".into()),
                username: None,
                kind: TgChatKind::Group,
                message_count: 0,
                last_date: None,
                last_text: String::new(),
            })
        })?;
        for r in rows.flatten() {
            chats.push(r);
        }
    }

    Ok(chats)
}

/// Estrae i messaggi per una specifica chat da Postbox o legacy.
pub fn messages(cx: &Ctx, chat_id: i64) -> Res<Vec<TgMessage>> {
    let mut msgs = Vec::new();

    let mut st = cx.m.prepare(
        "SELECT fileID FROM Files
         WHERE (domain = ?1 OR domain = ?2) AND (relativePath LIKE '%postbox/db/db_sqlite' OR relativePath LIKE '%tgdata.sqlite%')",
    )?;
    let db_files = st
        .query_map([GROUP_DOMAIN, APP_DOMAIN], |r| Ok(txt(r, 0)))?
        .collect::<Result<Vec<_>, _>>()?;

    for file_id in db_files {
        if let Ok(conn) = open_temp_sqlite(cx, &file_id) {
            let table_exists: bool = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='messages'",
                    [],
                    |r| Ok(r.get::<_, i64>(0)? > 0),
                )
                .unwrap_or(false);

            if table_exists {
                let mut m_st = conn.prepare(
                    "SELECT id, text, timestamp, from_id FROM messages WHERE peer_id = ?1 ORDER BY timestamp ASC LIMIT 5000",
                )?;
                let rows = m_st.query_map([chat_id], |r| {
                    Ok(TgMessage {
                        id: r.get(0)?,
                        chat_id,
                        from_name: r.get(3).unwrap_or_else(|_| "Utente".into()),
                        text: r.get(1).unwrap_or_default(),
                        date: r.get::<_, f64>(2).unwrap_or(0.0),
                        out: false,
                        media_desc: None,
                    })
                })?;
                for m in rows.flatten() {
                    msgs.push(m);
                }
            }
        }
    }

    Ok(msgs)
}

fn open_temp_sqlite(cx: &Ctx, file_id: &str) -> Res<Connection> {
    let content = cx.content(file_id)?;
    let bytes = content.read_all()?;
    mem_db(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_structs() {
        let meta = TelegramMeta::default();
        assert!(!meta.is_installed);
        assert_eq!(meta.app_files_count, 0);

        let chat = TgChat {
            id: 123456,
            title: "Test Chat".into(),
            username: Some("testuser".into()),
            kind: TgChatKind::Private,
            message_count: 5,
            last_date: Some(1700000000.0),
            last_text: "Ciao!".into(),
        };
        assert_eq!(chat.id, 123456);
        assert_eq!(chat.username.as_deref(), Some("testuser"));
    }
}
