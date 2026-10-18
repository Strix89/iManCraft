use crate::{blob_path, mem_db, open_db, txt, Ctx, Res, APPLE_EPOCH};

#[derive(Clone, Debug, Default)]
pub struct Reminder {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub list: String,
    pub completed: bool,
    pub due_date: Option<f64>,
    pub completion_date: Option<f64>,
    pub creation_date: Option<f64>,
    pub priority: u32,
}

/// Estrae tutti i promemoria (iOS 13+ da Reminders Stores e pre-iOS 13 da CalendarItem).
pub fn reminders(cx: &Ctx) -> Res<Vec<Reminder>> {
    let mut out = Vec::new();

    // 1. Modern iOS (13+): AppDomainGroup-group.com.apple.reminders -> Container_v1/Stores/Data-*.sqlite
    if let Ok(mut stmt) = cx.m.prepare("SELECT fileID, relativePath FROM Files WHERE domain = 'AppDomainGroup-group.com.apple.reminders' AND relativePath LIKE 'Container_v1/Stores/Data-%.sqlite'") {
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows.flatten() {
            let (file_id, _) = row;
            let conn = match cx.keys {
                None => open_db(&blob_path(cx.dir, &file_id)?),
                Some(_) => mem_db(cx.content(&file_id)?.read_all()?),
            };
            if let Ok(db) = conn {
                if let Ok(mut r_stmt) = db.prepare(
                    "SELECT r.Z_PK, r.ZTITLE, r.ZNOTES, l.ZNAME, r.ZCOMPLETED, r.ZDUEDATE, r.ZCOMPLETIONDATE, r.ZCREATIONDATE, r.ZPRIORITY
                     FROM ZREMCDREMINDER r
                     LEFT JOIN ZREMCDBASELIST l ON r.ZLIST = l.Z_PK
                     WHERE r.ZTITLE IS NOT NULL AND r.ZTITLE != ''
                     ORDER BY r.ZCOMPLETED ASC, r.ZDUEDATE ASC, r.ZCREATIONDATE DESC"
                ) {
                    let rems = r_stmt.query_map([], |row| {
                        let id = row.get(0)?;
                        let title = txt(row, 1);
                        let notes = txt(row, 2);
                        let list = txt(row, 3);
                        let completed = row.get::<_, i64>(4).unwrap_or(0) != 0;
                        let due_raw = row.get::<_, Option<f64>>(5).unwrap_or(None);
                        let comp_raw = row.get::<_, Option<f64>>(6).unwrap_or(None);
                        let creat_raw = row.get::<_, Option<f64>>(7).unwrap_or(None);
                        let priority = row.get::<_, u32>(8).unwrap_or(0);

                        Ok(Reminder {
                            id,
                            title,
                            notes,
                            list: if list.is_empty() { "Promemoria".to_string() } else { list },
                            completed,
                            due_date: due_raw.map(|t| t + APPLE_EPOCH),
                            completion_date: comp_raw.map(|t| t + APPLE_EPOCH),
                            creation_date: creat_raw.map(|t| t + APPLE_EPOCH),
                            priority,
                        })
                    })?;
                    for rem in rems.flatten() {
                        out.push(rem);
                    }
                }
            }
        }
    }

    // 2. Fallback per versioni più vecchie o promemoria salvati in Calendar.sqlitedb (entity_type = 1)
    if out.is_empty() {
        if let Ok(db) = cx.sqlite("HomeDomain", "Library/Calendar/Calendar.sqlitedb") {
            if let Ok(mut stmt) = db.prepare(
                "SELECT ci.ROWID, ci.summary, ci.description, c.title, ci.completion_date, ci.due_date, ci.priority, ci.status
                 FROM CalendarItem ci
                 LEFT JOIN Calendar c ON ci.calendar_id = c.ROWID
                 WHERE ci.entity_type = 1
                 ORDER BY ci.completion_date ASC, ci.due_date ASC"
            ) {
                let rems = stmt.query_map([], |row| {
                    let id = row.get(0)?;
                    let title = txt(row, 1);
                    let notes = txt(row, 2);
                    let list = txt(row, 3);
                    let comp_raw = row.get::<_, Option<f64>>(4).unwrap_or(None);
                    let due_raw = row.get::<_, Option<f64>>(5).unwrap_or(None);
                    let priority = row.get::<_, u32>(6).unwrap_or(0);
                    let status = row.get::<_, i64>(7).unwrap_or(0);

                    let completed = comp_raw.is_some() || status == 1;

                    Ok(Reminder {
                        id,
                        title,
                        notes,
                        list: if list.is_empty() { "Promemoria".to_string() } else { list },
                        completed,
                        due_date: due_raw.map(|t| t + APPLE_EPOCH),
                        completion_date: comp_raw.map(|t| t + APPLE_EPOCH),
                        creation_date: None,
                        priority,
                    })
                })?;
                for rem in rems.flatten() {
                    out.push(rem);
                }
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reminder_struct() {
        let r = Reminder {
            id: 1,
            title: "Fare la spesa".to_string(),
            notes: "Latte, uova".to_string(),
            list: "Spesa".to_string(),
            completed: false,
            due_date: Some(1700000000.0),
            completion_date: None,
            creation_date: Some(1699990000.0),
            priority: 1,
        };
        assert_eq!(r.title, "Fare la spesa");
        assert!(!r.completed);
    }
}
