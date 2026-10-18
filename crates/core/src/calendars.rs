//! Lettura dei calendari ed eventi iOS da `Calendar.sqlitedb`
//! ed esportazione in formato standard iCalendar (.ics, RFC 5545).

use crate::{num, txt, Ctx, Res, APPLE_EPOCH};

#[derive(Clone, Debug, Default)]
pub struct CalendarEvent {
    pub id: i64,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub start_date: f64,
    pub end_date: f64,
    pub all_day: bool,
    pub calendar_title: String,
    pub calendar_color: String,
}

/// Carica tutti gli eventi del calendario ordinati per data decrescente.
pub fn events(cx: &Ctx) -> Res<Vec<CalendarEvent>> {
    let db = cx.sqlite("HomeDomain", "Library/Calendar/Calendar.sqlitedb")
        .map_err(|e| format!("{e} (il database Calendar.sqlitedb non è presente nel backup)"))?;

    // Controlla se esiste la tabella Location
    let has_location = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='Location'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    let sql = if has_location {
        "SELECT i.ROWID, i.summary, i.description, coalesce(l.title, ''), i.start_date, i.end_date, i.all_day,
                coalesce(c.title, 'Predefinito'), coalesce(c.color, '')
         FROM CalendarItem i
         LEFT JOIN Calendar c ON c.ROWID = i.calendar_id
         LEFT JOIN Location l ON l.ROWID = i.location_id
         ORDER BY i.start_date DESC"
    } else {
        "SELECT i.ROWID, i.summary, i.description, '', i.start_date, i.end_date, i.all_day,
                coalesce(c.title, 'Predefinito'), coalesce(c.color, '')
         FROM CalendarItem i
         LEFT JOIN Calendar c ON c.ROWID = i.calendar_id
         ORDER BY i.start_date DESC"
    };

    let mut st = db.prepare(sql)?;
    let out = st
        .query_map([], |r| {
            let start = num(r, 4);
            let end = num(r, 5);
            Ok(CalendarEvent {
                id: num(r, 0) as i64,
                summary: txt(r, 1),
                description: txt(r, 2),
                location: txt(r, 3),
                start_date: if start != 0.0 { start + APPLE_EPOCH } else { 0.0 },
                end_date: if end != 0.0 { end + APPLE_EPOCH } else { 0.0 },
                all_day: num(r, 6) != 0.0,
                calendar_title: txt(r, 7),
                calendar_color: txt(r, 8),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(out)
}

/// Genera un file iCalendar (.ics) conforme a RFC 5545 a partire da un elenco di eventi.
pub fn export_ics(events: &[CalendarEvent]) -> String {
    let mut ics = String::new();
    ics.push_str("BEGIN:VCALENDAR\r\n");
    ics.push_str("VERSION:2.0\r\n");
    ics.push_str("PRODID:-//IManCraft//iOS Backup Calendar//IT\r\n");
    ics.push_str("CALSCALE:GREGORIAN\r\n");

    let fmt_dt = |ts: f64| -> String {
        let secs = ts as i64;
        let days = secs.div_euclid(86400);
        let rem = secs.rem_euclid(86400);
        let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

        // Algoritmo civile per data gregoriana
        let z = days + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let mon = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = y + (if mon <= 2 { 1 } else { 0 });

        format!("{y:04}{mon:02}{d:02}T{h:02}{m:02}{s:02}Z")
    };

    let escape_ics = |s: &str| -> String {
        s.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(',', "\\,")
            .replace('\n', "\\n")
            .replace('\r', "")
    };

    for ev in events {
        ics.push_str("BEGIN:VEVENT\r\n");
        ics.push_str(&format!("UID:imancraft-event-{}@imancraft.local\r\n", ev.id));
        ics.push_str(&format!("DTSTAMP:{}\r\n", fmt_dt(ev.start_date)));

        if ev.all_day {
            // Data semplice YYYYMMDD
            let dt = fmt_dt(ev.start_date);
            let date_only = &dt[..8];
            ics.push_str(&format!("DTSTART;VALUE=DATE:{date_only}\r\n"));
        } else {
            ics.push_str(&format!("DTSTART:{}\r\n", fmt_dt(ev.start_date)));
            if ev.end_date > ev.start_date {
                ics.push_str(&format!("DTEND:{}\r\n", fmt_dt(ev.end_date)));
            }
        }

        if !ev.summary.is_empty() {
            ics.push_str(&format!("SUMMARY:{}\r\n", escape_ics(&ev.summary)));
        }
        if !ev.description.is_empty() {
            ics.push_str(&format!("DESCRIPTION:{}\r\n", escape_ics(&ev.description)));
        }
        if !ev.location.is_empty() {
            ics.push_str(&format!("LOCATION:{}\r\n", escape_ics(&ev.location)));
        }
        if !ev.calendar_title.is_empty() {
            ics.push_str(&format!("CATEGORIES:{}\r\n", escape_ics(&ev.calendar_title)));
        }

        ics.push_str("END:VEVENT\r\n");
    }

    ics.push_str("END:VCALENDAR\r\n");
    ics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ics_export() {
        let ev = CalendarEvent {
            id: 42,
            summary: "Riunione di progetto; prova".into(),
            description: "Dettagli linea 1\nLinea 2".into(),
            location: "Ufficio 1, Piano 2".into(),
            start_date: 1704067200.0, // 2024-01-01 00:00:00 UTC
            end_date: 1704070800.0,
            all_day: false,
            calendar_title: "Lavoro".into(),
            calendar_color: "#FF0000".into(),
        };

        let ics = export_ics(&[ev]);
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("UID:imancraft-event-42@imancraft.local"));
        assert!(ics.contains("SUMMARY:Riunione di progetto\\; prova"));
        assert!(ics.contains("LOCATION:Ufficio 1\\, Piano 2"));
        assert!(ics.contains("CATEGORIES:Lavoro"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
    }
}
