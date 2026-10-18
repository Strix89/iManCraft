//! Lettura delle registrazioni audio (Memo Vocali) da CloudRecordings.db / Recordings.db.

use crate::{num, txt, Ctx, Res, APPLE_EPOCH};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct VoiceMemo {
    pub id: i64,
    pub title: String,
    pub filename: String,
    pub date: f64,
    pub duration: f64,
    pub audio_file_id: Option<String>,
}

/// Carica l'elenco dei memo vocali con i rispettivi file audio (.m4a).
pub fn voice_memos(cx: &Ctx) -> Res<Vec<VoiceMemo>> {
    // 1. Cerca il database dei memo vocali (dominio condiviso VoiceMemos)
    let domain = "AppDomainGroup-group.com.apple.VoiceMemos.shared";
    let db_id = cx.find(domain, "Recordings/CloudRecordings.db")
        .or_else(|| cx.find(domain, "Recordings/Recordings.db"))
        .or_else(|| cx.find("MediaDomain", "Recordings/Recordings.db"))
        .ok_or_else(|| "Nessun database di memo vocali trovato nel backup".to_string())?;

    // 2. Mappa dei file audio .m4a registrati nel Manifest per questo dominio
    let mut audio_map: HashMap<String, String> = HashMap::new();
    let mut st_audio = cx.m.prepare(
        "SELECT relativePath, fileID FROM Files
         WHERE domain = ?1 AND flags = 1 AND relativePath LIKE '%.m4a'",
    )?;
    let mut audio_rows = st_audio.query([domain])?;
    while let Some(r) = audio_rows.next()? {
        let rel = txt(r, 0);
        let id = txt(r, 1);
        let filename = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        audio_map.insert(filename, id);
    }

    // 3. Connessione SQLite
    let conn = match cx.keys {
        None => crate::open_db(&crate::blob_path(cx.dir, &db_id)?)?,
        Some(_) => crate::mem_db(cx.content(&db_id)?.read_all()?)?,
    };

    // Controlla se esiste ZCLOUDRECORDING (iOS recente) o ZRECORDING
    let has_cloud_rec = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='ZCLOUDRECORDING'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    let sql = if has_cloud_rec {
        "SELECT Z_PK, ZPATH, ZCUSTOMLABEL, ZDATE, ZDURATION FROM ZCLOUDRECORDING ORDER BY ZDATE DESC"
    } else {
        "SELECT Z_PK, ZPATH, ZCUSTOMLABEL, ZDATE, ZDURATION FROM ZRECORDING ORDER BY ZDATE DESC"
    };


    let mut st = conn.prepare(sql)?;
    let out = st
        .query_map([], |r| {
            let pk = num(r, 0) as i64;
            let path = txt(r, 1);
            let label = txt(r, 2);
            let d = num(r, 3);
            let dur = num(r, 4);

            let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
            let audio_file_id = audio_map.get(&filename).cloned();

            let title = if !label.is_empty() {
                label
            } else if !filename.is_empty() {
                filename.clone()
            } else {
                format!("Registrazione #{pk}")
            };

            Ok(VoiceMemo {
                id: pk,
                title,
                filename,
                date: if d != 0.0 { d + APPLE_EPOCH } else { 0.0 },
                duration: dur,
                audio_file_id,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(out)
}
