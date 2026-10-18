//! Backup Extractor (Esporta tutti i dati):
//! Esporta l'intero contenuto del backup in cartelle organizzate e file leggibili su disco
//! (Foto, Note, Contatti, Messaggi, Chiamate, Calendari, Voicemail, Memo vocali).

use crate::calendars::{self, export_ics};
use crate::data::{self, contacts, notes};
use crate::export::{export_calls_xlsx, export_contacts_vcf, export_contacts_xlsx, export_reminders_xlsx};
use crate::recordings;
use crate::reminders;
use crate::safari_bookmarks;
use crate::voicemail;
use crate::{Ctx, Res};
use std::fs;
use std::path::Path;


#[derive(Clone, Debug, Default)]
pub struct ExtractorSummary {
    pub contacts_count: usize,
    pub notes_count: usize,
    pub calls_count: usize,
    pub calendar_events_count: usize,
    pub reminders_count: usize,
    pub voicemail_count: usize,
    pub voice_memos_count: usize,
    pub bookmarks_count: usize,
}

/// Esporta tutti i dati disponibili nel backup all'interno della cartella di destinazione `out_dir`.
pub fn export_all_data(
    cx: &Ctx,
    out_dir: &Path,
    on_step: impl Fn(&str),
) -> Res<ExtractorSummary> {
    fs::create_dir_all(out_dir)?;
    let mut summary = ExtractorSummary::default();

    // 1. Contatti
    on_step("Esportazione Contatti...");
    if let Ok(c_list) = contacts(cx) {
        if !c_list.is_empty() {
            let contacts_dir = out_dir.join("Contatti");
            fs::create_dir_all(&contacts_dir)?;

            let vcf = export_contacts_vcf(&c_list);
            fs::write(contacts_dir.join("Contatti.vcf"), vcf)?;
            let _ = export_contacts_xlsx(&c_list, &contacts_dir.join("Contatti.xlsx"));
            summary.contacts_count = c_list.len();
        }
    }

    // 2. Note
    on_step("Esportazione Note...");
    if let Ok(n_list) = notes(cx) {
        if !n_list.is_empty() {
            let notes_dir = out_dir.join("Note");
            fs::create_dir_all(&notes_dir)?;

            for (idx, n) in n_list.iter().enumerate() {
                let folder_name = if n.folder.is_empty() { "Principale" } else { &n.folder };
                let target_sub = notes_dir.join(folder_name);
                let _ = fs::create_dir_all(&target_sub);

                let safe_title: String = n
                    .title
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
                    .collect();
                let file_title = if safe_title.trim().is_empty() {
                    format!("Nota_{idx}")
                } else {
                    safe_title.trim().to_string()
                };

                let note_content = format!("TITOLO: {}\nCARTELLA: {}\nBLOCCATA: {}\n\n{}", n.title, n.folder, n.locked, n.text);
                let _ = fs::write(target_sub.join(format!("{file_title}.txt")), note_content);
            }
            summary.notes_count = n_list.len();
        }
    }

    // 3. Chiamate (se backup cifrato)
    on_step("Esportazione Registro Chiamate...");
    if let Ok(c_list) = data::calls(cx) {
        if !c_list.is_empty() {
            let calls_dir = out_dir.join("Chiamate");
            fs::create_dir_all(&calls_dir)?;
            let _ = export_calls_xlsx(&c_list, &calls_dir.join("Chiamate.xlsx"));
            summary.calls_count = c_list.len();
        }
    }

    // 4. Calendari
    on_step("Esportazione Calendari...");
    if let Ok(ev_list) = calendars::events(cx) {
        if !ev_list.is_empty() {
            let cal_dir = out_dir.join("Calendario");
            fs::create_dir_all(&cal_dir)?;
            let ics_data = export_ics(&ev_list);
            let _ = fs::write(cal_dir.join("Calendario.ics"), ics_data);
            let _ = crate::export::export_calendar_xlsx(&ev_list, &cal_dir.join("Calendario.xlsx"));
            summary.calendar_events_count = ev_list.len();
        }
    }

    // 5. Segreteria Telefonica (Voicemail)
    on_step("Esportazione Segreteria...");
    if let Ok(vm_list) = voicemail::voicemails(cx) {
        if !vm_list.is_empty() {
            let vm_dir = out_dir.join("Segreteria");
            fs::create_dir_all(&vm_dir)?;

            let mut txt_summary = String::from("ID;Data;Durata(s);Mittente;Numero;FileAudio\n");
            for vm in &vm_list {
                let audio_name = if let Some(audio_id) = &vm.audio_file_id {
                    let dest_amr = vm_dir.join(format!("{}_{}.amr", vm.id, vm.sender));
                    if let Ok(c) = cx.content(audio_id) {
                        let _ = c.save_to(&dest_amr);
                    }
                    format!("{}_{}.amr", vm.id, vm.sender)
                } else {
                    String::from("-")
                };
                txt_summary.push_str(&format!("{};{};{};{};{};{}\n", vm.id, vm.date, vm.duration, vm.sender, vm.callback_num, audio_name));
            }
            let _ = fs::write(vm_dir.join("Segreteria_Indice.csv"), txt_summary);
            summary.voicemail_count = vm_list.len();
        }
    }

    // 6. Memo Vocali
    on_step("Esportazione Memo Vocali...");
    if let Ok(rec_list) = recordings::voice_memos(cx) {
        if !rec_list.is_empty() {
            let rec_dir = out_dir.join("MemoVocali");
            fs::create_dir_all(&rec_dir)?;

            for r in &rec_list {
                if let Some(audio_id) = &r.audio_file_id {
                    let safe_title: String = r
                        .title
                        .chars()
                        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
                        .collect();
                    let target_m4a = rec_dir.join(format!("{}.m4a", safe_title.trim()));
                    if let Ok(content) = cx.content(audio_id) {
                        let _ = content.save_to(&target_m4a);
                    }
                }
            }
            summary.voice_memos_count = rec_list.len();
        }
    }

    // 7. Preferiti Safari
    on_step("Esportazione Preferiti Safari...");
    if let Ok(bm_list) = safari_bookmarks::bookmarks(cx) {
        if !bm_list.is_empty() {
            let safari_dir = out_dir.join("Safari");
            fs::create_dir_all(&safari_dir)?;

            let mut html = String::from("<!DOCTYPE html><html><head><meta charset='utf-8'><title>Preferiti Safari</title></head><body><h1>Preferiti Safari</h1><ul>\n");
            for b in &bm_list {
                if !b.is_folder && !b.url.is_empty() {
                    html.push_str(&format!("  <li><a href='{}'>{}</a></li>\n", b.url, b.title));
                }
            }
            html.push_str("</ul></body></html>");
            let _ = fs::write(safari_dir.join("Preferiti.html"), html);
            summary.bookmarks_count = bm_list.len();
        }
    }

    // 8. Promemoria
    on_step("Esportazione Promemoria...");
    if let Ok(rem_list) = reminders::reminders(cx) {
        if !rem_list.is_empty() {
            let rem_dir = out_dir.join("Promemoria");
            fs::create_dir_all(&rem_dir)?;
            let _ = export_reminders_xlsx(&rem_list, &rem_dir.join("Promemoria.xlsx"));
            summary.reminders_count = rem_list.len();
        }
    }

    on_step("Esportazione completata.");
    Ok(summary)
}
