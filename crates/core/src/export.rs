//! Esportazione avanzata in formati standard:
//! - Fogli di calcolo Excel (.xlsx) tramite `rust_xlsxwriter` (Rust puro)
//! - Rubrica vCard (.vcf, RFC 6350)
//! - Calendari (.ics, RFC 5545)

use crate::calendars::CalendarEvent;
use crate::data::{Call, Contact, Message};
use crate::reminders::Reminder;
use crate::whatsapp::{self, ExportFormat, Filter};
use crate::{Ctx, Res};
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use std::fs;
use std::path::Path;

/// Esporta una chat WhatsApp completa con filtri in formato Excel (.xlsx).
pub fn export_whatsapp_xlsx(
    cx: &Ctx,
    chat_id: i64,
    f: &Filter,
    tz: i64,
    dest_path: &Path,
) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Messaggi WhatsApp")?;

    // Stile intestazione
    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x128C7E)) // Verde WhatsApp
        .set_border(FormatBorder::Thin);

    // Intestazioni colonne
    let headers = ["Data e Ora", "Mittente", "Tipo", "Messaggio", "Allegato / Media"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 20)?;
    worksheet.set_column_width(1, 24)?;
    worksheet.set_column_width(2, 12)?;
    worksheet.set_column_width(3, 50)?;
    worksheet.set_column_width(4, 30)?;

    // Recupera messaggi (sfruttando le query di whatsapp.rs)
    let (_, csv_text) = whatsapp::export(cx, chat_id, f, ExportFormat::Csv, tz)?;
    let mut row_idx = 1u32;
    for line in csv_text.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').map(|s| s.trim_matches('"')).collect();
        if parts.len() >= 5 {
            worksheet.write_string(row_idx, 0, parts[0])?;
            worksheet.write_string(row_idx, 1, parts[1])?;
            worksheet.write_string(row_idx, 2, parts[2])?;
            worksheet.write_string(row_idx, 3, parts[3])?;
            worksheet.write_string(row_idx, 4, parts[4])?;
            row_idx += 1;
        }
    }

    workbook.save(dest_path)?;
    Ok(())
}

/// Esporta il registro chiamate in formato Excel (.xlsx).
pub fn export_calls_xlsx(calls: &[Call], dest_path: &Path) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Registro Chiamate")?;

    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x007AFF)) // Blu iOS
        .set_border(FormatBorder::Thin);

    let headers = ["Data e Ora", "Contatto", "Numero / Indirizzo", "Direzione", "Esito", "Tipo", "Durata (s)", "Gestore"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 20)?;
    worksheet.set_column_width(1, 24)?;
    worksheet.set_column_width(2, 20)?;
    worksheet.set_column_width(3, 14)?;
    worksheet.set_column_width(4, 14)?;
    worksheet.set_column_width(5, 16)?;
    worksheet.set_column_width(6, 12)?;
    worksheet.set_column_width(7, 16)?;

    for (idx, c) in calls.iter().enumerate() {
        let row = (idx + 1) as u32;
        let dt_str = whatsapp::fmt_date(c.t, 0); // data locale / utc
        worksheet.write_string(row, 0, &dt_str)?;
        worksheet.write_string(row, 1, &c.who)?;
        worksheet.write_string(row, 2, &c.address)?;
        worksheet.write_string(row, 3, if c.out { "In uscita" } else { "In entrata" })?;
        worksheet.write_string(row, 4, if c.answered { "Risposta" } else { "Persa" })?;
        worksheet.write_string(row, 5, c.kind)?;
        worksheet.write_number(row, 6, c.duration)?;
        worksheet.write_string(row, 7, &c.provider)?;
    }

    workbook.save(dest_path)?;
    Ok(())
}

/// Esporta i contatti della rubrica in formato Excel (.xlsx).
pub fn export_contacts_xlsx(contacts: &[Contact], dest_path: &Path) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Contatti")?;

    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x5856D6))
        .set_border(FormatBorder::Thin);

    let headers = ["Nome Completo", "Organizzazione / Azienda", "Numeri di Telefono", "Email"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 26)?;
    worksheet.set_column_width(1, 26)?;
    worksheet.set_column_width(2, 35)?;
    worksheet.set_column_width(3, 35)?;

    for (idx, c) in contacts.iter().enumerate() {
        let row = (idx + 1) as u32;
        worksheet.write_string(row, 0, &c.name)?;
        worksheet.write_string(row, 1, &c.org)?;

        let phones = c.phones.iter().map(|(l, v)| format!("{l}: {v}")).collect::<Vec<_>>().join(", ");
        worksheet.write_string(row, 2, &phones)?;

        let emails = c.emails.iter().map(|(l, v)| format!("{l}: {v}")).collect::<Vec<_>>().join(", ");
        worksheet.write_string(row, 3, &emails)?;
    }

    workbook.save(dest_path)?;
    Ok(())
}

/// Esporta i contatti in formato vCard 3.0 (.vcf).
pub fn export_contacts_vcf(contacts: &[Contact]) -> String {
    let mut out = String::new();
    for c in contacts {
        out.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
        out.push_str(&format!("FN:{}\r\n", c.name));
        if !c.org.is_empty() {
            out.push_str(&format!("ORG:{}\r\n", c.org));
        }
        for (label, num) in &c.phones {
            let pref = if label.to_lowercase().contains("cell") || label.to_lowercase().contains("mob") {
                ";TYPE=CELL"
            } else if label.to_lowercase().contains("work") || label.to_lowercase().contains("lav") {
                ";TYPE=WORK"
            } else {
                ";TYPE=HOME"
            };
            out.push_str(&format!("TEL{pref}:{}\r\n", num));
        }
        for (_, email) in &c.emails {
            out.push_str(&format!("EMAIL;TYPE=INTERNET:{}\r\n", email));
        }
        out.push_str("END:VCARD\r\n");
    }
    out
}

/// Esporta i messaggi di una chat SMS / iMessage in formato Excel (.xlsx).
pub fn export_messages_xlsx(chat_title: &str, messages: &[Message], dest_path: &Path) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    let sheet_name: String = chat_title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .take(30)
        .collect();
    let name = if sheet_name.trim().is_empty() { "Messaggi" } else { sheet_name.trim() };
    worksheet.set_name(name)?;



    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x34C759)) // Verde messaggi
        .set_border(FormatBorder::Thin);

    let headers = ["Data e Ora", "Mittente", "Direzione", "Testo", "Allegati"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 20)?;
    worksheet.set_column_width(1, 24)?;
    worksheet.set_column_width(2, 14)?;
    worksheet.set_column_width(3, 50)?;
    worksheet.set_column_width(4, 30)?;

    for (idx, m) in messages.iter().enumerate() {
        let row = (idx + 1) as u32;
        let dt_str = whatsapp::fmt_date(m.t, 0);
        worksheet.write_string(row, 0, &dt_str)?;
        worksheet.write_string(row, 1, if m.me { "Io" } else { &m.from })?;
        worksheet.write_string(row, 2, if m.me { "Inviato" } else { "Ricevuto" })?;
        worksheet.write_string(row, 3, &m.text)?;

        let atts = m.att.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ");
        worksheet.write_string(row, 4, &atts)?;
    }

    workbook.save(dest_path)?;
    Ok(())
}

/// Esporta gli eventi del calendario in formato Excel (.xlsx).
pub fn export_calendar_xlsx(events: &[CalendarEvent], dest_path: &Path) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Calendario")?;

    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0xFF3B30)) // Rosso Calendario Apple
        .set_border(FormatBorder::Thin);

    let headers = ["Titolo Evento", "Calendario", "Inizio", "Fine", "Tutto il Giorno", "Luogo", "Note"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 30)?;
    worksheet.set_column_width(1, 18)?;
    worksheet.set_column_width(2, 20)?;
    worksheet.set_column_width(3, 20)?;
    worksheet.set_column_width(4, 16)?;
    worksheet.set_column_width(5, 26)?;
    worksheet.set_column_width(6, 40)?;

    for (idx, ev) in events.iter().enumerate() {
        let row = (idx + 1) as u32;
        worksheet.write_string(row, 0, &ev.summary)?;
        worksheet.write_string(row, 1, &ev.calendar_title)?;
        worksheet.write_string(row, 2, &whatsapp::fmt_date(ev.start_date, 0))?;
        worksheet.write_string(row, 3, &whatsapp::fmt_date(ev.end_date, 0))?;
        worksheet.write_string(row, 4, if ev.all_day { "Sì" } else { "No" })?;
        worksheet.write_string(row, 5, &ev.location)?;
        worksheet.write_string(row, 6, &ev.description)?;
    }

    workbook.save(dest_path)?;
    Ok(())
}

/// Esporta i promemoria in formato Excel (.xlsx).
pub fn export_reminders_xlsx(reminders: &[Reminder], dest_path: &Path) -> Res<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet.set_name("Promemoria")?;

    let header_format = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0xFF9500)) // Arancione Promemoria Apple
        .set_border(FormatBorder::Thin);

    let headers = ["Stato", "Titolo Promemoria", "Elenco", "Scadenza", "Completato il", "Priorità", "Note"];
    for (col, text) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, col as u16, *text, &header_format)?;
    }

    worksheet.set_column_width(0, 14)?;
    worksheet.set_column_width(1, 35)?;
    worksheet.set_column_width(2, 20)?;
    worksheet.set_column_width(3, 20)?;
    worksheet.set_column_width(4, 20)?;
    worksheet.set_column_width(5, 12)?;
    worksheet.set_column_width(6, 40)?;

    for (idx, r) in reminders.iter().enumerate() {
        let row = (idx + 1) as u32;
        worksheet.write_string(row, 0, if r.completed { "Completato" } else { "Da fare" })?;
        worksheet.write_string(row, 1, &r.title)?;
        worksheet.write_string(row, 2, &r.list)?;
        worksheet.write_string(row, 3, &r.due_date.map(|t| whatsapp::fmt_date(t, 0)).unwrap_or_default())?;
        worksheet.write_string(row, 4, &r.completion_date.map(|t| whatsapp::fmt_date(t, 0)).unwrap_or_default())?;
        worksheet.write_string(row, 5, match r.priority {
            1..=4 => "Alta",
            5 => "Media",
            6..=9 => "Bassa",
            _ => "Nessuna",
        })?;
        worksheet.write_string(row, 6, &r.notes)?;
    }

    workbook.save(dest_path)?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Esporta una conversazione iMessage/SMS in formato HTML autonomo pronto per stampa/PDF.
pub fn export_messages_html(title: &str, messages: &[Message], dest_path: &Path) -> Res<()> {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"it\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str(&format!("<title>Conversazione: {}</title>\n", html_escape(title)));
    html.push_str(r#"<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; background: #F2F2F7; margin: 0; padding: 24px; color: #1C1C1E; }
.container { max-width: 760px; margin: 0 auto; background: #FFFFFF; border-radius: 14px; box-shadow: 0 4px 16px rgba(0,0,0,0.08); overflow: hidden; }
.header { background: #007AFF; color: #FFFFFF; padding: 20px 24px; }
.header h1 { margin: 0 0 6px 0; font-size: 20px; font-weight: 600; }
.header .meta { font-size: 13px; opacity: 0.9; }
.chat { padding: 24px; display: flex; flex-direction: column; gap: 12px; }
.msg-wrap { display: flex; flex-direction: column; max-width: 75%; }
.msg-wrap.me { align-self: flex-end; align-items: flex-end; }
.msg-wrap.other { align-self: flex-start; align-items: flex-start; }
.sender { font-size: 11px; color: #8E8E93; margin-bottom: 2px; padding: 0 4px; font-weight: 500; }
.bubble { padding: 10px 14px; border-radius: 18px; font-size: 14px; line-height: 1.4; word-break: break-word; }
.me .bubble { background: #007AFF; color: #FFFFFF; border-bottom-right-radius: 4px; }
.other .bubble { background: #E9E9EB; color: #000000; border-bottom-left-radius: 4px; }
.time { font-size: 10px; opacity: 0.75; margin-top: 4px; text-align: right; }
.att { margin-top: 6px; padding: 6px 10px; background: rgba(0,0,0,0.06); border-radius: 8px; font-size: 12px; display: inline-block; }
.me .att { background: rgba(255,255,255,0.2); }
@media print {
  body { background: #FFFFFF; padding: 0; }
  .container { box-shadow: none; border-radius: 0; max-width: 100%; }
}
</style>
</head>
<body>
<div class="container">
"#);
    html.push_str(&format!(
        "<div class=\"header\"><h1>{}</h1><div class=\"meta\">{} messaggi · Esportato da IManCraft</div></div>\n<div class=\"chat\">\n",
        html_escape(title),
        messages.len()
    ));

    for m in messages {
        let cls = if m.me { "me" } else { "other" };
        let sender = if m.me { "Io" } else { &m.from };
        let time_str = whatsapp::fmt_date(m.t, 0);

        html.push_str(&format!("<div class=\"msg-wrap {}\">\n", cls));
        if !m.me && !sender.is_empty() {
            html.push_str(&format!("<div class=\"sender\">{}</div>\n", html_escape(sender)));
        }
        html.push_str("<div class=\"bubble\">\n");
        if !m.text.is_empty() {
            html.push_str(&html_escape(&m.text).replace('\n', "<br>"));
        }
        for a in &m.att {
            html.push_str(&format!("<div class=\"att\">📎 {}</div>", html_escape(&a.name)));
        }
        html.push_str(&format!("<div class=\"time\">{}</div>\n", html_escape(&time_str)));
        html.push_str("</div>\n</div>\n");
    }

    html.push_str("</div>\n</div>\n</body>\n</html>\n");
    fs::write(dest_path, html)?;
    Ok(())
}

/// Esporta una chat WhatsApp in formato HTML autonomo pronto per stampa/PDF.
pub fn export_whatsapp_html(title: &str, cx: &Ctx, chat_id: i64, f: &Filter, tz: i64, dest_path: &Path) -> Res<()> {
    let (_, csv_text) = whatsapp::export(cx, chat_id, f, ExportFormat::Csv, tz)?;
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"it\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str(&format!("<title>WhatsApp: {}</title>\n", html_escape(title)));
    html.push_str(r#"<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; background: #ECE5DD; margin: 0; padding: 24px; color: #111B21; }
.container { max-width: 760px; margin: 0 auto; background: #EFEAE2; border-radius: 14px; box-shadow: 0 4px 16px rgba(0,0,0,0.1); overflow: hidden; }
.header { background: #008069; color: #FFFFFF; padding: 18px 24px; }
.header h1 { margin: 0 0 4px 0; font-size: 20px; font-weight: 600; }
.header .meta { font-size: 13px; opacity: 0.9; }
.chat { padding: 24px; display: flex; flex-direction: column; gap: 10px; }
.msg-wrap { display: flex; flex-direction: column; max-width: 75%; }
.msg-wrap.me { align-self: flex-end; align-items: flex-end; }
.msg-wrap.other { align-self: flex-start; align-items: flex-start; }
.bubble { padding: 8px 12px; border-radius: 8px; font-size: 14px; line-height: 1.4; word-break: break-word; box-shadow: 0 1px 2px rgba(0,0,0,0.12); }
.me .bubble { background: #D9FDD3; border-top-right-radius: 0; }
.other .bubble { background: #FFFFFF; border-top-left-radius: 0; }
.sender { font-size: 12px; font-weight: bold; color: #128C7E; margin-bottom: 2px; }
.time { font-size: 10px; color: #667781; margin-top: 4px; text-align: right; }
.att { margin-top: 4px; font-size: 12px; color: #54656F; font-style: italic; }
@media print {
  body { background: #FFFFFF; padding: 0; }
  .container { box-shadow: none; border-radius: 0; max-width: 100%; }
}
</style>
</head>
<body>
<div class="container">
"#);
    html.push_str(&format!(
        "<div class=\"header\"><h1>{}</h1><div class=\"meta\">Chat WhatsApp · Esportato da IManCraft</div></div>\n<div class=\"chat\">\n",
        html_escape(title)
    ));

    for line in csv_text.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(';').map(|s| s.trim_matches('"')).collect();
        if parts.len() >= 4 {
            let dt = parts[0];
            let sender = parts[1];
            let kind = parts[2];
            let text = parts[3];
            let att = if parts.len() >= 5 { parts[4] } else { "" };

            let is_me = sender == "Io";
            let cls = if is_me { "me" } else { "other" };

            html.push_str(&format!("<div class=\"msg-wrap {}\">\n<div class=\"bubble\">\n", cls));
            if !is_me && !sender.is_empty() {
                html.push_str(&format!("<div class=\"sender\">{}</div>\n", html_escape(sender)));
            }
            if !text.is_empty() {
                html.push_str(&html_escape(text).replace('\n', "<br>"));
            }
            if !att.is_empty() {
                html.push_str(&format!("<div class=\"att\">[{}: {}]</div>\n", html_escape(kind), html_escape(att)));
            }
            html.push_str(&format!("<div class=\"time\">{}</div>\n", html_escape(dt)));
            html.push_str("</div>\n</div>\n");
        }
    }

    html.push_str("</div>\n</div>\n</body>\n</html>\n");
    fs::write(dest_path, html)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vcf_export() {
        let c = Contact {
            name: "Mario Rossi".into(),
            org: "Acme SpA".into(),
            phones: vec![("Cellulare".into(), "+393331234567".into())],
            emails: vec![("Lavoro".into(), "mario@acme.it".into())],
        };
        let vcf = export_contacts_vcf(&[c]);
        assert!(vcf.contains("BEGIN:VCARD"));
        assert!(vcf.contains("FN:Mario Rossi"));
        assert!(vcf.contains("ORG:Acme SpA"));
        assert!(vcf.contains("TEL;TYPE=CELL:+393331234567"));
        assert!(vcf.contains("EMAIL;TYPE=INTERNET:mario@acme.it"));
        assert!(vcf.ends_with("END:VCARD\r\n"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<b>\"Ciao\" & 'Mondo'</b>"), "&lt;b&gt;&quot;Ciao&quot; &amp; &#39;Mondo&#39;&lt;/b&gt;");
    }
}
