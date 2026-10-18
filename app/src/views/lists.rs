//! Viste a elenco: Contatti, Note, Chiamate, Safari, Calendari, Segreteria, Spyware, Backup Extractor.

use super::{filter_box, fmt_date, header, save_dialog, Env};
use crate::player::fmt_time;
use crate::tasks::{self, Task};
use egui::RichText;
use egui_extras::{Column, TableBuilder};
use imancraft_core::calendars::{self, CalendarEvent};
use imancraft_core::data::{self, Call, Contact, Note, Visit};
use imancraft_core::export::{
    export_calendar_xlsx, export_calls_xlsx, export_contacts_vcf, export_contacts_xlsx, export_reminders_xlsx,
};
use imancraft_core::extractor::{self, ExtractorSummary};
use imancraft_core::recordings::{self, VoiceMemo};
use imancraft_core::reminders::{self, Reminder};
use imancraft_core::safari_bookmarks::{self, SafariBookmark};
use imancraft_core::search::{self, SearchResult};
use imancraft_core::spyware::{self, SpywareFinding};
use imancraft_core::telegram::{self, TelegramStatus, TgChat, TgMessage};
use imancraft_core::voicemail::{self, VoicemailItem};
use std::collections::HashMap;
use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{channel, Receiver},
    Arc,
};

#[derive(Default)]
pub struct State {
    contacts: Option<Task<Vec<Contact>>>,
    notes: Option<Task<Vec<Note>>>,
    calls: Option<Task<Vec<Call>>>,
    safari: Option<Task<Vec<Visit>>>,
    bookmarks: Option<Task<Vec<SafariBookmark>>>,
    calendars: Option<Task<Vec<CalendarEvent>>>,
    reminders: Option<Task<Vec<Reminder>>>,
    voicemails: Option<Task<Vec<VoicemailItem>>>,
    voice_memos: Option<Task<Vec<VoiceMemo>>>,
    spyware: Option<Task<Vec<SpywareFinding>>>,
    extractor_job: Option<Task<ExtractorSummary>>,
    export_action: Option<Task<()>>,
    filter: String,
    note_sel: Option<usize>,
    note_pwd_input: String,
    note_unlock_status: Option<Result<String, String>>,
    note_unlocked_texts: HashMap<i64, String>,
    note_cracking: bool,
    #[allow(dead_code)]
    note_crack_rx: Option<Receiver<imancraft_core::notes_recovery::CrackProgress>>,
    note_crack_cancel: Option<Arc<AtomicBool>>,
    note_crack_status: Option<(usize, usize, f32)>,
    note_found_pwd: Option<String>,
    note_pin_digits: usize,
    safari_tab: usize, // 0 = Cronologia, 1 = Preferiti
    voicemail_tab: usize, // 0 = Segreteria, 1 = Memo vocali
    reminder_filter: usize, // 0 = Tutti, 1 = Da fare, 2 = Completati
    search_query: String,
    search_task: Option<Task<Vec<SearchResult>>>,
    telegram_status: Option<Task<TelegramStatus>>,
    telegram_chat_sel: Option<i64>,
    telegram_msgs: Option<Task<Vec<TgMessage>>>,
}


/// Intestazioni di tabella monospazio.
pub fn head(cols: &[&str], mut h: egui_extras::TableRow) {
    for t in cols {
        h.col(|ui| {
            ui.weak(RichText::new(*t).monospace().size(10.5));
        });
    }
}

impl State {
    pub fn contacts(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "06", "Rubrica", "Contatti");
        if self.contacts.is_none() {
            self.contacts = Some(tasks::load(ui.ctx(), "Contatti", env.backup, data::contacts));
        }
        let Some(list) = tasks::show(ui, self.contacts.as_ref()) else { return };

        ui.horizontal(|ui| {
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} contatti…", list.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⬇ XLSX (Excel)").on_hover_text("Esporta contatti in foglio Excel").clicked() {
                    if let Some(dest) = save_dialog("Contatti.xlsx") {
                        let contacts_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione contatti Excel", move || {
                            export_contacts_xlsx(&contacts_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
                if ui.button("⬇ vCard (.vcf)").on_hover_text("Esporta contatti in formato standard vCard").clicked() {
                    if let Some(dest) = save_dialog("Contatti.vcf") {
                        let contacts_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione contatti vCard", move || {
                            let vcf_data = export_contacts_vcf(&contacts_clone);
                            fs::write(&dest, vcf_data).map_err(|e| e.to_string())?;
                            Ok(())
                        }));
                    }
                }
            });
        });
        ui.add_space(4.0);

        let f = self.filter.to_lowercase();
        let rows: Vec<&Contact> = list
            .iter()
            .filter(|c| f.is_empty() || format!("{} {} {:?} {:?}", c.name, c.org, c.phones, c.emails).to_lowercase().contains(&f))
            .collect();
        let vals = |v: &[(String, String)]| v.iter().map(|(l, x)| if l.is_empty() { x.clone() } else { format!("{x}  ({l})") }).collect::<Vec<_>>().join("\n");
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(260.0))
            .column(Column::initial(260.0))
            .column(Column::remainder())
            .header(22.0, |h| head(&["NOME", "TELEFONO", "EMAIL"], h))
            .body(|body| {
                body.heterogeneous_rows(rows.iter().map(|c| 22.0 * c.phones.len().max(c.emails.len()).max(1) as f32 + 4.0), |mut row| {
                    let c = rows[row.index()];
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(if c.name.is_empty() { "(senza nome)" } else { &c.name }).strong());
                            if !c.org.is_empty() && c.org != c.name {
                                ui.weak(&c.org);
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.label(vals(&c.phones));
                    });
                    row.col(|ui| {
                        ui.label(vals(&c.emails));
                    });
                });
            });
    }

    fn start_note_crack(&mut self, backup: Arc<imancraft_core::Backup>, candidates: Vec<String>) {
        if candidates.is_empty() {
            self.note_unlock_status = Some(Err("Nessun candidato da verificare.".into()));
            return;
        }
        let Ok((verifier, _)) = backup.notes_crypto_context() else {
            self.note_unlock_status = Some(Err("Impossibile caricare i parametri di cifratura delle Note.".into()));
            return;
        };
        let Some(v) = verifier else {
            self.note_unlock_status = Some(Err("Nessun verificatore crittografico trovato nel database NoteStore.sqlite.".into()));
            return;
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        self.note_cracking = true;
        self.note_crack_rx = Some(rx);
        self.note_crack_cancel = Some(cancel.clone());
        self.note_crack_status = Some((0, candidates.len(), 0.0));
        self.note_unlock_status = None;

        let salt = v.salt;
        let iters = v.iterations;
        let verifier_data = v.verifier;

        std::thread::spawn(move || {
            imancraft_core::notes_recovery::run_crack_session(
                salt,
                iters,
                verifier_data,
                candidates,
                cancel,
                tx,
            );
        });
    }

    fn start_note_pin_crack(&mut self, backup: Arc<imancraft_core::Backup>, digits: usize) {
        let Ok((verifier, _)) = backup.notes_crypto_context() else {
            self.note_unlock_status = Some(Err("Impossibile caricare i parametri di cifratura delle Note.".into()));
            return;
        };
        let Some(v) = verifier else {
            self.note_unlock_status = Some(Err("Nessun verificatore crittografico trovato nel database NoteStore.sqlite.".into()));
            return;
        };

        let digits = digits.clamp(1, 10);
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        self.note_cracking = true;
        self.note_crack_rx = Some(rx);
        self.note_crack_cancel = Some(cancel.clone());
        let total_estimate = 10_usize.pow(digits.min(9) as u32);
        self.note_crack_status = Some((0, total_estimate, 0.0));
        self.note_unlock_status = None;

        let salt = v.salt;
        let iters = v.iterations;
        let verifier_data = v.verifier;

        std::thread::spawn(move || {
            imancraft_core::notes_recovery::run_pin_crack_session(
                salt,
                iters,
                verifier_data,
                digits,
                cancel,
                tx,
            );
        });
    }

    fn unlock_note_manual(&mut self, backup: &imancraft_core::Backup, note_id: i64, password: &str) {
        let pwd = password.trim();
        if pwd.is_empty() {
            self.note_unlock_status = Some(Err("Inserisci una password valida.".into()));
            return;
        }

        let Ok((verifier, protected_notes)) = backup.notes_crypto_context() else {
            self.note_unlock_status = Some(Err("Impossibile accedere al database delle Note.".into()));
            return;
        };

        let Some(target) = protected_notes.iter().find(|pn| pn.note_pk == note_id) else {
            self.note_unlock_status = Some(Err(format!("Nota {note_id} non trovata tra i record protetti.")));
            return;
        };

        match imancraft_core::notes_recovery::unlock_note(target, verifier.as_ref(), pwd) {
            Ok(decrypted) => {
                self.note_unlocked_texts.insert(note_id, decrypted);
                self.note_found_pwd = Some(pwd.to_string());
                let mut count = 1;
                // Sblocca in cascata tutte le altre note protette
                for pn in &protected_notes {
                    if pn.note_pk != note_id {
                        if let Ok(text) = imancraft_core::notes_recovery::unlock_note(pn, verifier.as_ref(), pwd) {
                            self.note_unlocked_texts.insert(pn.note_pk, text);
                            count += 1;
                        }
                    }
                }
                self.note_unlock_status = Some(Ok(format!("✓ Sbloccata con successo! {count} note decifrate con la password '{pwd}'.")));
            }
            Err(e) => {
                self.note_unlock_status = Some(Err(format!("Password errata: {e}")));
            }
        }
    }

    pub fn notes(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        // Polling messaggi di progresso dalla sessione di cracking asincrona
        let mut crack_event = None;
        if let Some(ref rx) = self.note_crack_rx {
            while let Ok(msg) = rx.try_recv() {
                crack_event = Some(msg);
            }
        }
        if let Some(msg) = crack_event {
            match msg {
                imancraft_core::notes_recovery::CrackProgress::Running { tried, total, speed } => {
                    self.note_crack_status = Some((tried, total, speed));
                    ui.ctx().request_repaint();
                }
                imancraft_core::notes_recovery::CrackProgress::Found { password, tried } => {
                    self.note_cracking = false;
                    self.note_crack_rx = None;
                    self.note_crack_cancel = None;
                    self.note_found_pwd = Some(password.clone());
                    self.note_unlock_status = Some(Ok(format!("✓ Password trovata con successo dopo {tried} tentativi: '{password}'!")));

                    // Sblocca tutte le note protette caricate
                    if let Ok((verifier, protected_notes)) = env.backup.notes_crypto_context() {
                        for pn in &protected_notes {
                            if let Ok(text) = imancraft_core::notes_recovery::unlock_note(pn, verifier.as_ref(), &password) {
                                self.note_unlocked_texts.insert(pn.note_pk, text);
                            }
                        }
                    }
                    ui.ctx().request_repaint();
                }
                imancraft_core::notes_recovery::CrackProgress::DoneNotFound { total } => {
                    self.note_cracking = false;
                    self.note_crack_rx = None;
                    self.note_crack_cancel = None;
                    self.note_unlock_status = Some(Err(format!("Nessuna password trovata tra i {total} candidati testati. Prova con una wordlist personalizzata (.txt).")));
                    ui.ctx().request_repaint();
                }
                imancraft_core::notes_recovery::CrackProgress::Cancelled => {
                    self.note_cracking = false;
                    self.note_crack_rx = None;
                    self.note_crack_cancel = None;
                    self.note_unlock_status = Some(Err("Sessione di recupero interrotta dall'utente.".into()));
                    ui.ctx().request_repaint();
                }
            }
        }

        if self.notes.is_none() {
            self.notes = Some(tasks::load(ui.ctx(), "Note", env.backup, data::notes));
        }
        let p = *env.p;
        super::side_panel("notes-list", env.p).show(ui, |ui| {
            header(ui, env.p, "07", "Note", "Note");
            let Some(list) = tasks::show(ui, self.notes.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca in {} note…", list.len()));
            let f = self.filter.to_lowercase();
            let filtered: Vec<(usize, &Note)> = list
                .iter()
                .enumerate()
                .filter(|(_, n)| f.is_empty() || format!("{} {}", n.title, n.text).to_lowercase().contains(&f))
                .collect();
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 52.0, filtered.len(), |ui, range| {
                for &(i, n) in &filtered[range] {
                    let is_sel = self.note_sel == Some(i);
                    let is_unlocked = self.note_unlocked_texts.contains_key(&n.id);
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), egui::Sense::click());
                    if is_sel {
                        ui.painter().rect_filled(rect, 4.0, p.accent.gamma_multiply(0.18));
                        ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, p.accent), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 4.0, p.panel_alt);
                    }
                    let pt = ui.painter_at(rect);
                    let lock_icon = if is_unlocked {
                        "🔓 "
                    } else if n.locked {
                        "🔒 "
                    } else {
                        ""
                    };
                    let title = format!("{}{}", lock_icon, if n.title.is_empty() { "(senza titolo)" } else { &n.title });
                    let max_title_chars = ((rect.width() - 20.0) / 7.5).max(10.0) as usize;
                    pt.text(
                        egui::pos2(rect.min.x + 8.0, rect.min.y + 6.0),
                        egui::Align2::LEFT_TOP,
                        super::short(&title, max_title_chars),
                        egui::FontId::proportional(13.0),
                        if is_unlocked { egui::Color32::from_rgb(120, 220, 140) } else { p.text },
                    );
                    pt.text(
                        egui::pos2(rect.min.x + 8.0, rect.min.y + 26.0),
                        egui::Align2::LEFT_TOP,
                        format!("{} · {}", n.folder, fmt_date(n.modified)),
                        egui::FontId::proportional(11.0),
                        p.muted,
                    );
                    ui.add_space(2.0);
                    if resp.clicked() {
                        self.note_sel = Some(i);
                        self.note_unlock_status = None;
                    }
                }
            });
        });

        let Some(n) = self.note_sel.and_then(|i| self.notes.as_ref()?.ready()?.as_ref().ok()?.get(i).cloned()) else {
            ui.weak("Seleziona una nota dall'elenco a sinistra.");
            return;
        };


        let is_unlocked = self.note_unlocked_texts.contains_key(&n.id);
        crate::theme::title(ui, &n.title);
        ui.weak(format!("{} · {}{}", n.folder, fmt_date(n.modified), if n.locked && !is_unlocked { " · nota protetta da password" } else if is_unlocked { " · protetta [sbloccata]" } else { "" }));

        if n.locked && !is_unlocked {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(p.panel_alt)
                .stroke(egui::Stroke::new(1.0, p.accent))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::theme::chip(ui, "🔒 NOTA PROTETTA", egui::Color32::from_rgb(240, 160, 40));
                        ui.label(RichText::new("Cifratura Apple iOS (PBKDF2-SHA256 / AES-GCM)").strong().color(p.text));
                    });

                    if let Some(ref h) = n.hint {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("💡 Indizio Password:").strong().color(p.accent));
                            ui.label(RichText::new(format!("«{h}»")).italics().color(p.text));
                        });
                    }

                    ui.add_space(8.0);
                    // Sblocco manuale diretto
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Password:").color(p.muted));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.note_pwd_input)
                                .password(true)
                                .desired_width(220.0)
                                .hint_text("Inserisci password…"),
                        );
                        let enter_hit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (ui.button(RichText::new("🔓 Sblocca").strong()).clicked() || enter_hit) && !self.note_pwd_input.is_empty() {
                            let pwd = self.note_pwd_input.clone();
                            self.unlock_note_manual(&env.backup, n.id, &pwd);
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Strumenti di recupero automatico / brute-force multi-core
                    if self.note_pin_digits == 0 {
                        self.note_pin_digits = 4;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Recupero Multi-Core:").small().color(p.muted));
                        if ui.button("⚡ Attacco da Indizio").on_hover_text("Genera e verifica permutazioni intelligenti basate sull'indizio (maiuscole, accenti, suffissi, parole singole) e password comuni").clicked() && !self.note_cracking {
                            let hint = n.hint.as_deref().unwrap_or("");
                            let mut cands = imancraft_core::notes_recovery::generate_hint_candidates(hint);
                            cands.extend(imancraft_core::notes_recovery::common_passwords());
                            self.start_note_crack(env.backup.clone(), cands);
                        }

                        ui.separator();
                        ui.label(RichText::new("Cifre PIN:").small().color(p.muted));
                        ui.add(egui::DragValue::new(&mut self.note_pin_digits).range(3..=10).speed(0.1));
                        let digits = self.note_pin_digits;
                        if ui.button(format!("🔢 Attacco PIN ({digits} cifre)")).on_hover_text(format!("Brute-force parallelo multi-core per PIN a {digits} cifre (date e pattern intelligenti prioritari)")).clicked() && !self.note_cracking {
                            self.start_note_pin_crack(env.backup.clone(), digits);
                        }

                        ui.separator();
                        if ui.button("📂 Wordlist (.txt)…").on_hover_text("Carica un file dizionario personalizzato (.txt, .dict, .lst) per eseguire il brute-force").clicked() && !self.note_cracking {
                            if let Some(path) = rfd::FileDialog::new().add_filter("Wordlist", &["txt", "dict", "lst"]).pick_file() {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let cands: Vec<String> = content.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                                    self.start_note_crack(env.backup.clone(), cands);
                                }
                            }
                        }
                    });

                    // Progresso cracking in corso
                    if self.note_cracking {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            if let Some((tried, total, speed)) = self.note_crack_status {
                                let pct = (tried as f32 / total.max(1) as f32 * 100.0).min(100.0);
                                ui.label(RichText::new(format!("Recupero in corso: {tried} / {total} ({pct:.1}%) · ~{speed:.0} pwd/sec")).color(p.accent));
                            }
                            if ui.button("⏹ Interrompi").clicked() {
                                if let Some(ref c) = self.note_crack_cancel {
                                    c.store(true, Ordering::Relaxed);
                                }
                            }
                        });
                        if let Some((tried, total, _)) = self.note_crack_status {
                            ui.add(egui::ProgressBar::new(tried as f32 / total.max(1) as f32).animate(true));
                        }
                    }

                    // Messaggio di stato
                    if let Some(ref st) = self.note_unlock_status {
                        ui.add_space(6.0);
                        match st {
                            Ok(msg) => {
                                ui.label(RichText::new(msg).color(egui::Color32::from_rgb(100, 220, 120)));
                            }
                            Err(err) => {
                                ui.label(RichText::new(format!("⚠ {err}")).color(egui::Color32::from_rgb(240, 120, 100)));
                            }
                        }
                    }
                });
            ui.add_space(8.0);
            ui.label(RichText::new("Anteprima non protetta recuperata dal backup:").small().color(p.muted));
        } else if is_unlocked {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(18, 42, 24))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 180, 80)))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::theme::chip(ui, "🔓 SBLOCCATA", egui::Color32::from_rgb(50, 180, 80));
                        ui.label(RichText::new("Contenuto integrale decifrato con successo").strong().color(egui::Color32::WHITE));
                        if let Some(ref pwd) = self.note_found_pwd {
                            ui.weak(format!("(Password: '{}')", pwd));
                        }
                    });
                });
            ui.add_space(8.0);
        }

        ui.separator();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            if let Some(decrypted_text) = self.note_unlocked_texts.get(&n.id) {
                ui.add(egui::Label::new(decrypted_text).selectable(true));
            } else {
                ui.add(egui::Label::new(&n.text).selectable(true));
            }
        });
    }


    pub fn calls(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "08", "Registro chiamate", "Chiamate");
        if self.calls.is_none() {
            self.calls = Some(tasks::load(ui.ctx(), "Chiamate", env.backup, data::calls));
        }
        let Some(list) = tasks::show(ui, self.calls.as_ref()) else { return };

        ui.horizontal(|ui| {
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} chiamate…", list.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⬇ XLSX (Excel)").on_hover_text("Esporta registro chiamate in Excel").clicked() {
                    if let Some(dest) = save_dialog("RegistroChiamate.xlsx") {
                        let calls_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione chiamate Excel", move || {
                            export_calls_xlsx(&calls_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
            });
        });
        ui.add_space(4.0);

        let f = self.filter.to_lowercase();
        let rows: Vec<&Call> = list.iter().filter(|c| f.is_empty() || format!("{} {}", c.who, c.address).to_lowercase().contains(&f)).collect();
        let p = *env.p;
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::exact(28.0))
            .column(Column::initial(280.0))
            .column(Column::initial(200.0))
            .column(Column::initial(150.0))
            .column(Column::remainder())
            .header(22.0, |h| head(&["", "CONTATTO", "TIPO", "DATA", "DURATA"], h))
            .body(|body| {
                body.rows(24.0, rows.len(), |mut row| {
                    let c = rows[row.index()];
                    row.col(|ui| {
                        if c.out {
                            ui.label("↗");
                        } else if c.answered {
                            ui.label("↙");
                        } else {
                            ui.colored_label(p.danger, "✗");
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&c.who).strong()).on_hover_text(&c.address);
                    });
                    row.col(|ui| {
                        ui.label(format!("{}{}", c.kind, if c.kind == "App" && !c.provider.is_empty() { format!(" ({})", c.provider) } else { String::new() }));
                        if !c.out && !c.answered {
                            ui.colored_label(p.danger, "persa");
                        }
                    });
                    row.col(|ui| {
                        ui.weak(fmt_date(c.t));
                    });
                    row.col(|ui| {
                        ui.weak(fmt_time(c.duration));
                    });
                });
            });
    }

    pub fn safari(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "09", "Safari e Web", "Safari");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.safari_tab, 0, "Cronologia Visite");
            ui.selectable_value(&mut self.safari_tab, 1, "Preferiti & Elenco Lettura");
        });
        ui.add_space(4.0);

        if self.safari_tab == 0 {
            if self.safari.is_none() {
                self.safari = Some(tasks::load(ui.ctx(), "Cronologia Safari", env.backup, data::safari));
            }
            let Some(list) = tasks::show(ui, self.safari.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca in {} visite…", list.len()));
            let f = self.filter.to_lowercase();
            let rows: Vec<&Visit> = list.iter().filter(|v| f.is_empty() || format!("{} {}", v.title, v.url).to_lowercase().contains(&f)).collect();
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::exact(140.0))
                .column(Column::remainder())
                .header(22.0, |h| head(&["DATA", "PAGINA"], h))
                .body(|body| {
                    body.rows(40.0, rows.len(), |mut row| {
                        let v = rows[row.index()];
                        row.col(|ui| {
                            ui.weak(fmt_date(v.t));
                        });
                        row.col(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(if v.title.is_empty() { &v.url } else { &v.title }).strong());
                                if v.url.starts_with("http://") || v.url.starts_with("https://") {
                                    ui.hyperlink_to(RichText::new(&v.url).small(), &v.url);
                                } else {
                                    ui.weak(&v.url);
                                }
                            });
                        });
                    });
                });
        } else {
            if self.bookmarks.is_none() {
                self.bookmarks = Some(tasks::load(ui.ctx(), "Preferiti Safari", env.backup, safari_bookmarks::bookmarks));
            }
            let Some(list) = tasks::show(ui, self.bookmarks.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} preferiti…", list.len()));
            let f = self.filter.to_lowercase();
            let rows: Vec<&SafariBookmark> = list.iter().filter(|b| f.is_empty() || format!("{} {}", b.title, b.url).to_lowercase().contains(&f)).collect();
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::exact(36.0))
                .column(Column::initial(240.0))
                .column(Column::remainder())
                .header(22.0, |h| head(&["", "TITOLO", "INDIRIZZO / CARTELLA"], h))
                .body(|body| {
                    body.rows(32.0, rows.len(), |mut row| {
                        let b = rows[row.index()];
                        row.col(|ui| {
                            ui.label(if b.is_folder { "📁" } else if b.is_reading_list { "📖" } else { "🔖" });
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&b.title).strong());
                        });
                        row.col(|ui| {
                            if b.is_folder {
                                ui.weak("Cartella segnalibri");
                            } else if b.url.starts_with("http://") || b.url.starts_with("https://") {
                                ui.hyperlink_to(RichText::new(&b.url).small(), &b.url);
                            } else {
                                ui.weak(&b.url);
                            }
                        });
                    });
                });
        }
    }

    pub fn calendars(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "10", "Calendari e Promemoria", "Calendari");
        if self.calendars.is_none() {
            self.calendars = Some(tasks::load(ui.ctx(), "Calendari", env.backup, calendars::events));
        }
        let Some(list) = tasks::show(ui, self.calendars.as_ref()) else { return };

        ui.horizontal(|ui| {
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} eventi…", list.len()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⬇ XLSX (Excel)").on_hover_text("Esporta calendario in Excel").clicked() {
                    if let Some(dest) = save_dialog("Calendario.xlsx") {
                        let ev_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione calendario Excel", move || {
                            export_calendar_xlsx(&ev_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
                if ui.button("⬇ iCalendar (.ics)").on_hover_text("Esporta eventi in formato standard .ics").clicked() {
                    if let Some(dest) = save_dialog("Calendario.ics") {
                        let ev_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione calendario ICS", move || {
                            let ics_text = calendars::export_ics(&ev_clone);
                            fs::write(&dest, ics_text).map_err(|e| e.to_string())?;
                            Ok(())
                        }));
                    }
                }
            });
        });
        ui.add_space(4.0);

        let f = self.filter.to_lowercase();
        let rows: Vec<&CalendarEvent> = list
            .iter()
            .filter(|e| f.is_empty() || format!("{} {} {} {}", e.summary, e.description, e.location, e.calendar_title).to_lowercase().contains(&f))
            .collect();

        TableBuilder::new(ui)
            .striped(true)
            .column(Column::initial(180.0))
            .column(Column::initial(260.0))
            .column(Column::initial(160.0))
            .column(Column::remainder())
            .header(22.0, |h| head(&["DATA E ORA", "TITOLO EVENTO", "CALENDARIO", "LUOGO / NOTE"], h))
            .body(|body| {
                body.rows(36.0, rows.len(), |mut row| {
                    let ev = rows[row.index()];
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.label(fmt_date(ev.start_date));
                            if ev.all_day {
                                ui.weak("Tutto il giorno");
                            }
                        });
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&ev.summary).strong());
                    });
                    row.col(|ui| {
                        ui.weak(&ev.calendar_title);
                    });
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            if !ev.location.is_empty() {
                                ui.label(format!("📍 {}", ev.location));
                            }
                            if !ev.description.is_empty() {
                                ui.weak(super::short(&ev.description, 60));
                            }
                        });
                    });
                });
            });
    }

    pub fn voicemail(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "11", "Segreteria e Memo", "Segreteria & Memo vocali");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.voicemail_tab, 0, "Segreteria Telefonica (Voicemail)");
            ui.selectable_value(&mut self.voicemail_tab, 1, "Memo Vocali (Registratore)");
        });
        ui.add_space(4.0);

        if self.voicemail_tab == 0 {
            if self.voicemails.is_none() {
                self.voicemails = Some(tasks::load(ui.ctx(), "Segreteria", env.backup, voicemail::voicemails));
            }
            let Some(list) = tasks::show(ui, self.voicemails.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} messaggi…", list.len()));
            let f = self.filter.to_lowercase();
            let rows: Vec<&VoicemailItem> = list.iter().filter(|v| f.is_empty() || format!("{} {}", v.sender, v.callback_num).to_lowercase().contains(&f)).collect();

            TableBuilder::new(ui)
                .striped(true)
                .column(Column::initial(160.0))
                .column(Column::initial(220.0))
                .column(Column::exact(100.0))
                .column(Column::remainder())
                .header(22.0, |h| head(&["DATA", "MITTENTE", "DURATA", "AUDIO"], h))
                .body(|body| {
                    body.rows(30.0, rows.len(), |mut row| {
                        let vm = rows[row.index()];
                        row.col(|ui| {
                            ui.weak(fmt_date(vm.date));
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&vm.sender).strong());
                        });
                        row.col(|ui| {
                            ui.weak(fmt_time(vm.duration as f64));
                        });
                        row.col(|ui| {
                            if let Some(audio_id) = &vm.audio_file_id {
                                if ui.button("💾 Salva .amr").clicked() {
                                    super::export_file(ui.ctx(), env.backup, audio_id, &format!("Segreteria_{}.amr", vm.sender));
                                }
                            } else {
                                ui.weak("Audio non incluso");
                            }
                        });
                    });
                });
        } else {
            if self.voice_memos.is_none() {
                self.voice_memos = Some(tasks::load(ui.ctx(), "Memo vocali", env.backup, recordings::voice_memos));
            }
            let Some(list) = tasks::show(ui, self.voice_memos.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} registrazioni…", list.len()));
            let f = self.filter.to_lowercase();
            let rows: Vec<&VoiceMemo> = list.iter().filter(|r| f.is_empty() || format!("{} {}", r.title, r.filename).to_lowercase().contains(&f)).collect();

            TableBuilder::new(ui)
                .striped(true)
                .column(Column::initial(160.0))
                .column(Column::initial(300.0))
                .column(Column::exact(100.0))
                .column(Column::remainder())
                .header(22.0, |h| head(&["DATA", "TITOLO REGISTRAZIONE", "DURATA", "AUDIO"], h))
                .body(|body| {
                    body.rows(30.0, rows.len(), |mut row| {
                        let rec = rows[row.index()];
                        row.col(|ui| {
                            ui.weak(fmt_date(rec.date));
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&rec.title).strong());
                        });
                        row.col(|ui| {
                            ui.weak(fmt_time(rec.duration));
                        });
                        row.col(|ui| {
                            if let Some(audio_id) = &rec.audio_file_id {
                                if ui.button("💾 Salva .m4a").clicked() {
                                    super::export_file(ui.ctx(), env.backup, audio_id, &format!("{}.m4a", rec.title));
                                }
                            } else {
                                ui.weak("Audio non presente");
                            }
                        });
                    });
                });
        }
    }

    pub fn spyware(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "12", "Sicurezza e Spyware", "Rilevamento Spyware (MVT)");
        ui.label("Esegue la scansione offline del backup per identificare tracce o Indicatori di Compromissione (IOC) di spyware di sorveglianza mirata (Pegasus di NSO Group, Predator di Cytrox, ecc.).");
        ui.add_space(8.0);

        if self.spyware.is_none() {
            if ui.button("🛡 Avvia Scansione Sicurezza").clicked() {
                self.spyware = Some(tasks::load(ui.ctx(), "Scansione spyware IOC", env.backup, spyware::scan_spyware));
            }
            return;
        }

        let Some(findings) = tasks::show(ui, self.spyware.as_ref()) else { return };

        if findings.is_empty() {
            ui.add_space(8.0);
            crate::theme::card(env.p).show(ui, |ui| {
                ui.label(RichText::new("✅ Nessun indicatore malevolo rilevato").strong().size(15.0).color(env.p.success));
                ui.weak("La scansione su messaggi, cronologia web e database non ha trovato tracce o domini noti associati a Pegasus, Predator o spyware commerciale.");
            });
        } else {
            ui.colored_label(env.p.danger, format!("⚠ RILEVATI {} POSSIBILI INDICATORI DI COMPROMISSIONE", findings.len()));
            for f in findings {
                crate::theme::card(env.p).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(f.threat).strong().color(env.p.danger));
                        ui.weak(format!("Fonte: {}", f.source));
                    });
                    ui.label(&f.details);
                });
            }
        }
    }

    pub fn extractor(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "13", "Strumenti Backup", "Backup Extractor");
        ui.label("Esporta l'intero contenuto del backup in cartelle ordinate e leggibili su disco (Contatti, Note, Calendari, Segreteria, Memo vocali, Safari, Chiamate).");
        ui.add_space(10.0);

        let busy = self.extractor_job.as_ref().is_some_and(|t| t.ready().is_none());
        if ui.add_enabled(!busy, egui::Button::new("📁 Scegli cartella ed esporta tutti i dati…")).clicked() {
            if let Some(target_dir) = rfd::FileDialog::new().set_title("Seleziona cartella di destinazione").pick_folder() {
                self.extractor_job = Some(tasks::load(ui.ctx(), "Backup Extractor", env.backup, move |cx| {
                    extractor::export_all_data(cx, &target_dir, |_| {})
                }));
            }
        }

        if let Some(job) = &self.extractor_job {
            match job.ready() {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Esportazione in corso...");
                    });
                }
                Some(Ok(s)) => {
                    ui.add_space(8.0);
                    crate::theme::card(env.p).show(ui, |ui| {
                        ui.label(RichText::new("✅ Esportazione completata con successo!").strong().color(env.p.success));
                        ui.weak(format!(
                            "Contatti: {} · Note: {} · Chiamate: {} · Eventi: {} · Promemoria: {} · Segreteria: {} · Memo: {} · Preferiti: {}",
                            s.contacts_count, s.notes_count, s.calls_count, s.calendar_events_count, s.reminders_count, s.voicemail_count, s.voice_memos_count, s.bookmarks_count
                        ));
                    });
                }
                Some(Err(e)) => {
                    ui.colored_label(env.p.danger, format!("Errore durante l'esportazione: {e}"));
                }
            }
        }
    }

    pub fn reminders(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "11", "Attività e promemoria", "Promemoria");
        if self.reminders.is_none() {
            self.reminders = Some(tasks::load(ui.ctx(), "Promemoria", env.backup, reminders::reminders));
        }
        let Some(list) = tasks::show(ui, self.reminders.as_ref()) else { return };

        ui.horizontal(|ui| {
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} promemoria…", list.len()));
            ui.add_space(8.0);
            for (idx, label) in ["Tutti", "Da completare", "Completati"].iter().enumerate() {
                if ui.selectable_label(self.reminder_filter == idx, *label).clicked() {
                    self.reminder_filter = idx;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⬇ XLSX (Excel)").on_hover_text("Esporta promemoria in Excel").clicked() {
                    if let Some(dest) = save_dialog("Promemoria.xlsx") {
                        let rem_clone = list.clone();
                        self.export_action = Some(tasks::run(ui.ctx(), "Esportazione promemoria Excel", move || {
                            export_reminders_xlsx(&rem_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
            });
        });
        ui.add_space(4.0);

        let f = self.filter.to_lowercase();
        let rows: Vec<&Reminder> = list
            .iter()
            .filter(|r| match self.reminder_filter {
                1 => !r.completed,
                2 => r.completed,
                _ => true,
            })
            .filter(|r| f.is_empty() || format!("{} {} {}", r.title, r.notes, r.list).to_lowercase().contains(&f))
            .collect();

        TableBuilder::new(ui)
            .striped(true)
            .column(Column::exact(36.0))
            .column(Column::initial(300.0))
            .column(Column::initial(150.0))
            .column(Column::initial(150.0))
            .column(Column::initial(90.0))
            .column(Column::remainder())
            .header(22.0, |h| head(&["STATO", "TITOLO", "ELENCO", "SCADENZA", "PRIORITÀ", "NOTE"], h))
            .body(|body| {
                body.rows(26.0, rows.len(), |mut row| {
                    let r = rows[row.index()];
                    row.col(|ui| {
                        if r.completed {
                            ui.colored_label(env.p.success, "✓");
                        } else {
                            ui.weak("○");
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(&r.title).strong());
                    });
                    row.col(|ui| {
                        crate::theme::chip(ui, &r.list, env.p.accent);
                    });
                    row.col(|ui| {
                        ui.weak(r.due_date.map(fmt_date).unwrap_or_default());
                    });
                    row.col(|ui| {
                        let (p_text, p_col) = match r.priority {
                            1..=4 => ("Alta", env.p.danger),
                            5 => ("Media", env.p.accent),
                            6..=9 => ("Bassa", env.p.muted),
                            _ => ("-", env.p.muted),
                        };
                        ui.colored_label(p_col, p_text);
                    });
                    row.col(|ui| {
                        ui.weak(&r.notes);
                    });
                });
            });
    }

    pub fn search(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "02", "Ricerca unificata", "Ricerca Globale");
        ui.label("Cerca contemporaneamente in Contatti, Note, Calendari, Promemoria, Chat WhatsApp e Messaggi SMS/iMessage.");
        ui.add_space(8.0);

        let mut perform_search = false;
        ui.horizontal(|ui| {
            let resp = ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Cerca parola chiave, nome, telefono...").desired_width(360.0));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                perform_search = true;
            }
            if ui.button("🔎 Cerca").clicked() {
                perform_search = true;
            }
        });

        if perform_search && !self.search_query.trim().is_empty() {
            let q = self.search_query.trim().to_string();
            self.search_task = Some(tasks::load(ui.ctx(), "Ricerca globale", env.backup, move |cx| {
                search::global_search(cx, &q)
            }));
        }

        ui.separator();

        let Some(task) = &self.search_task else {
            ui.weak("Digita una parola e premi Cerca per visualizzare i risultati correlati.");
            return;
        };

        let Some(results) = tasks::show(ui, Some(task)) else { return };

        if results.is_empty() {
            ui.add_space(8.0);
            ui.weak("Nessun elemento corrispondente trovato nel backup.");
            return;
        }

        ui.label(RichText::new(format!("Trovati {} risultati:", results.len())).strong());
        ui.add_space(4.0);

        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 50.0, results.len(), |ui, range| {
            for res in &results[range] {
                crate::theme::card(env.p).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::theme::chip(ui, res.category, env.p.accent);
                        ui.label(RichText::new(&res.title).strong().size(14.0));
                        if let Some(d) = res.date {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.weak(fmt_date(d));
                            });
                        }
                    });
                    if !res.subtitle.is_empty() {
                        ui.weak(&res.subtitle);
                    }
                });
                ui.add_space(2.0);
            }
        });
    }

    pub fn telegram(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "08b", "Messaggistica", "Telegram");
        if self.telegram_status.is_none() {
            self.telegram_status = Some(tasks::load(ui.ctx(), "Analisi forense Telegram", env.backup, telegram::status));
        }

        let Some(status) = tasks::show(ui, self.telegram_status.as_ref()) else { return };

        match status {
            TelegramStatus::NotInstalled => {
                ui.add_space(16.0);
                crate::theme::card(env.p).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::theme::chip(ui, "NON INSTALLATO", env.p.muted);
                        ui.label(RichText::new("Telegram non trovato").strong().size(16.0));
                    });
                    ui.add_space(6.0);
                    ui.weak("L'applicazione Telegram (ph.telegra.Telegraph) non risulta installata o presente in questo backup iOS.");
                });
            }
            TelegramStatus::InstalledExcludedFromBackup(meta) => {
                ui.add_space(8.0);
                crate::theme::card(env.p).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::theme::chip(ui, "APP INSTALLATA", env.p.accent);
                        ui.label(RichText::new("Telegram per iOS (ph.telegra.Telegraph)").strong().size(16.0));
                    });
                    ui.add_space(10.0);
                    egui::Grid::new("tg_forensic_meta").spacing([24.0, 8.0]).show(ui, |ui| {
                        ui.weak("File totali nel backup:");
                        ui.label(RichText::new(format!("{}", meta.app_files_count)).monospace());
                        ui.end_row();

                        ui.weak("Container App Group:");
                        let group_str = if meta.has_group_container { "group.ph.telegra.Telegraph (Presente)" } else { "Assente" };
                        ui.label(RichText::new(group_str).monospace());
                        ui.end_row();

                        if let Some(t) = meta.startup_time {
                            ui.weak("Ultimo avvio registrato (CKStartup):");
                            ui.label(RichText::new(fmt_date(t as f64)).monospace());
                            ui.end_row();
                        }

                        if let Some(t) = meta.cache_index_timestamp {
                            ui.weak("Timestamp indice cache:");
                            ui.label(RichText::new(fmt_date(t as f64)).monospace());
                            ui.end_row();
                        }

                        ui.weak("Raggruppamento media:");
                        ui.label(if meta.media_grouping_disabled { "Disabilitato" } else { "Abilitato" });
                        ui.end_row();

                        if let Some(preset) = meta.preferred_video_preset {
                            ui.weak("Preset video preferito:");
                            ui.label(format!("{preset}"));
                            ui.end_row();
                        }
                    });

                    if !meta.databases_found.is_empty() {
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("Database e indici rilevati nel backup ({}):", meta.databases_found.len())).strong());
                        ui.add_space(4.0);
                        for db in &meta.databases_found {
                            ui.horizontal(|ui| {
                                ui.weak("•");
                                ui.label(RichText::new(db).monospace().size(11.0));
                            });
                        }
                    }

                    ui.add_space(14.0);
                    egui::Frame::new()
                        .fill(env.p.panel_alt)
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(12))
                        .stroke(egui::Stroke::new(1.0, env.p.accent))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("ℹ NOTA FORENSE APPLE / TELEGRAM").strong().color(env.p.accent));
                            });
                            ui.add_space(6.0);
                            ui.label("Telegram per iOS memorizza il database SQLite delle conversazioni e dei canali (Postbox) nella directory Documents/postbox/db/db_sqlite.");
                            ui.label("Per architettura ufficiale, Telegram applica programmaticamente l'attributo iOS NSURLIsExcludedFromBackupKey = true su tale cartella per non saturare lo spazio di archiviazione iCloud e iTunes/Finder.");
                            ui.label("Di conseguenza, i normali backup locali iOS contengono preferenze, token e file di cache, ma non la cronologia completa dei messaggi cloud, a meno di acquisizioni forensi complete (Full File System).");
                        });
                });
            }
            TelegramStatus::Available { meta: _, chats } => {
                let p = *env.p;
                ui.horizontal(|ui| {
                    filter_box(ui, &mut self.filter, &format!("Cerca tra {} chat…", chats.len()));
                    ui.add_space(8.0);
                    crate::theme::chip(ui, &format!("{} CHAT", chats.len()), env.p.accent);
                });
                ui.add_space(6.0);

                let f = self.filter.to_lowercase();
                let filtered: Vec<&TgChat> = chats
                    .iter()
                    .filter(|c| f.is_empty() || c.title.to_lowercase().contains(&f) || c.username.as_deref().unwrap_or("").to_lowercase().contains(&f))
                    .collect();

                let mut sel_chat_id = self.telegram_chat_sel;

                ui.columns(2, |cols| {
                    // Colonna sinistra: Elenco chat
                    egui::ScrollArea::vertical().id_salt("tg_chats_scroll").show(&mut cols[0], |ui| {
                        for c in &filtered {
                            let is_sel = sel_chat_id == Some(c.id);
                            let bg = if is_sel { p.panel_alt } else { egui::Color32::TRANSPARENT };
                            let r = egui::Frame::new()
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let kind_tag = match c.kind {
                                            telegram::TgChatKind::Private => "👤",
                                            telegram::TgChatKind::Group => "👥",
                                            telegram::TgChatKind::Supergroup => "📢",
                                            telegram::TgChatKind::Channel => "📡",
                                        };
                                        ui.label(RichText::new(kind_tag).size(14.0));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&c.title).strong());
                                            if let Some(ref u) = c.username {
                                                ui.weak(format!("@{u}"));
                                            }
                                        });
                                    });
                                })
                                .response;
                            if r.interact(egui::Sense::click()).clicked() {
                                sel_chat_id = Some(c.id);
                            }
                        }
                    });

                    // Colonna destra: Messaggi chat selezionata
                    if let Some(cid) = sel_chat_id {
                        if self.telegram_chat_sel != Some(cid) || self.telegram_msgs.is_none() {
                            self.telegram_chat_sel = Some(cid);
                            self.telegram_msgs = Some(tasks::load(cols[1].ctx(), "Messaggi Telegram", env.backup, move |cx| {
                                telegram::messages(cx, cid)
                            }));
                        }

                        if let Some(task) = &self.telegram_msgs {
                            if let Some(msgs) = tasks::show(&mut cols[1], Some(task)) {
                                if msgs.is_empty() {
                                    cols[1].weak("Nessun messaggio presente in questa chat.");
                                } else {
                                    egui::ScrollArea::vertical().id_salt("tg_msgs_scroll").show(&mut cols[1], |ui| {
                                        for m in msgs {
                                            super::bubble(ui, &p, m.out, m.id, |ui| {
                                                if !m.from_name.is_empty() && !m.out {
                                                    ui.label(RichText::new(&m.from_name).size(11.0).color(p.accent).strong());
                                                }
                                                ui.add(egui::Label::new(&m.text).wrap());
                                                if m.date > 0.0 {
                                                    ui.weak(RichText::new(fmt_date(m.date)).size(10.0));
                                                }
                                            });
                                            ui.add_space(4.0);
                                        }
                                    });
                                }
                            }
                        }
                    } else {
                        cols[1].weak("Seleziona una chat a sinistra per leggerne i messaggi.");
                    }
                });
            }
        }
    }
}
