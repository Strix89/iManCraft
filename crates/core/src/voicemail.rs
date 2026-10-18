use crate::{num, txt, Ctx, Res};

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct VoicemailItem {
    pub id: i64,
    pub date: f64,
    pub duration: i64,
    pub sender: String,
    pub callback_num: String,
    /// fileID del file audio .amr nel backup
    pub audio_file_id: Option<String>,
    pub trashed: bool,
}

/// Carica tutti i messaggi di segreteria presenti nel backup con i rispettivi file audio.
pub fn voicemails(cx: &Ctx) -> Res<Vec<VoicemailItem>> {
    // 1. Cerca il file voicemail.db in HomeDomain
    let mut st = cx.m.prepare(
        "SELECT fileID, relativePath FROM Files
         WHERE domain='HomeDomain' AND relativePath LIKE 'Library/Voicemail/%voicemail.db' LIMIT 1",
    )?;
    let db_row = st.query_row([], |r| Ok((txt(r, 0), txt(r, 1)))).ok();

    let (db_id, db_rel) = db_row.ok_or_else(|| "Nessun voicemail.db trovato nel backup".to_string())?;

    // Prefisso cartella del database per trovare gli audio corrispondenti
    let parent_dir = db_rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("Library/Voicemail");

    // 2. Mappa di tutti i file audio .amr in HomeDomain: relativePath -> fileID
    let mut audio_map: HashMap<String, String> = HashMap::new();
    let mut st_audio = cx.m.prepare(
        "SELECT relativePath, fileID FROM Files
         WHERE domain='HomeDomain' AND relativePath LIKE 'Library/Voicemail/%.amr'",
    )?;
    let mut audio_rows = st_audio.query([])?;
    while let Some(r) = audio_rows.next()? {
        audio_map.insert(txt(r, 0), txt(r, 1));
    }

    // 3. Connessione SQLite (memoria per cifrati o diretta)
    let conn = match cx.keys {
        None => crate::open_db(&crate::blob_path(cx.dir, &db_id)?)?,
        Some(_) => crate::mem_db(cx.content(&db_id)?.read_all()?)?,
    };

    let mut st_v = conn.prepare(
        "SELECT ROWID, date, duration, sender, callback_num, trashed_date
         FROM voicemail ORDER BY date DESC",
    )?;

    let out = st_v
        .query_map([], |r| {
            let row_id = num(r, 0) as i64;
            let d = num(r, 1);
            let trashed = num(r, 5) > 0.0;

            // Cerca il file audio: prima nel subfolder del db, poi come fallback in Library/Voicemail/<id>.amr
            let direct_path = format!("{parent_dir}/{row_id}.amr");
            let fallback_path = format!("Library/Voicemail/{row_id}.amr");
            let audio_file_id = audio_map.get(&direct_path).or_else(|| audio_map.get(&fallback_path)).cloned();

            Ok(VoicemailItem {
                id: row_id,
                date: d, // Unix timestamp in secondi
                duration: num(r, 2) as i64,
                sender: txt(r, 3),
                callback_num: txt(r, 4),
                audio_file_id,
                trashed,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(out)
}
