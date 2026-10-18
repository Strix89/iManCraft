//! Lettura dei preferiti e dell'elenco lettura di Safari da `Bookmarks.db`.

use crate::{num, txt, Ctx, Res};

#[derive(Clone, Debug, Default)]
pub struct SafariBookmark {
    pub id: i64,
    pub parent_id: i64,
    pub is_folder: bool,
    pub title: String,
    pub url: String,
    pub order_index: i64,
    pub is_reading_list: bool,
}

/// Carica tutti i segnalibri, cartelle e voci dell'elenco lettura di Safari.
pub fn bookmarks(cx: &Ctx) -> Res<Vec<SafariBookmark>> {
    let db = cx.sqlite("HomeDomain", "Library/Safari/Bookmarks.db")
        .map_err(|e| format!("{e} (il database Bookmarks.db non è presente nel backup)"))?;

    let mut st = db.prepare(
        "SELECT id, parent, type, title, url, order_index, special_id
         FROM bookmarks
         WHERE coalesce(deleted, 0) = 0 AND (hidden = 0 OR special_id > 0)
         ORDER BY parent, order_index",
    )?;

    let out = st
        .query_map([], |r| {
            let b_type = num(r, 2) as i64;
            let special_id = num(r, 6) as i64;

            Ok(SafariBookmark {
                id: num(r, 0) as i64,
                parent_id: num(r, 1) as i64,
                is_folder: b_type == 1,
                title: txt(r, 3),
                url: txt(r, 4),
                order_index: num(r, 5) as i64,
                is_reading_list: special_id == 2,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(out)
}
