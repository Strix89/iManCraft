//! Dispositivi: schede dei backup trovati (stile "Devices" di iMazing), dettagli e sblocco dei cifrati.

use super::{fmt_size, header};
use crate::tasks::{self, Task};
use crate::theme::{self, Palette};
use egui::{RichText, Sense, Stroke};
use imancraft_core::Backup;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Default)]
pub struct State {
    pw: String,
    unlocking: Option<Task<()>>,
    info: Option<(PathBuf, Vec<(String, Vec<(String, String)>)>)>,
}

pub fn ui(ui: &mut egui::Ui, p: &Palette, backups: &[Arc<Backup>], current: Option<usize>, scanning: bool, st: &mut State, pick: &mut Option<usize>) {
    header(ui, p, "01", "Dispositivi e backup", "Dispositivi");
    if scanning {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak("Ricerca dei backup…");
        });
    } else if backups.is_empty() {
        ui.weak("Nessun backup trovato nelle cartelle standard. Usa File → Aggiungi cartella di backup…");
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true), |ui| {
            for (i, b) in backups.iter().enumerate() {
                let sel = current == Some(i);
                let r = theme::card(p)
                    .stroke(Stroke::new(if sel { 1.5 } else { 1.0 }, if sel { p.accent } else { p.line }))
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("📱").size(30.0));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&b.summary.name).strong());
                                    ui.weak(format!("{} · iOS {}", b.summary.model, b.summary.ios));
                                    ui.weak(format!("{}{}", b.summary.date.get(..10).unwrap_or(&b.summary.date), if b.summary.size > 0 { format!(" · {}", fmt_size(b.summary.size)) } else { String::new() }));
                                });
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if b.encrypted {
                                    theme::chip(ui, if b.is_locked() { "cifrato" } else { "sbloccato" }, if b.is_locked() { p.danger } else { p.success });
                                } else {
                                    theme::chip(ui, "non cifrato", p.muted);
                                }
                                if b.dir.join("DDNABackup.plist").exists() {
                                    theme::chip(ui, "imazing", p.accent);
                                }
                            });
                        });
                    })
                    .response
                    .interact(Sense::click());
                if r.on_hover_text(b.dir.display().to_string()).clicked() {
                    *pick = Some(i);
                }
            }
        });
        ui.add_space(14.0);
        let Some(b) = current.and_then(|i| backups.get(i)) else { return };
        if b.is_locked() {
            unlock_box(ui, p, b, st);
            return;
        }
        if st.info.as_ref().map(|i| &i.0) != Some(&b.dir) {
            st.info = Some((b.dir.clone(), b.info()));
        }
        if let Some(ddna) = b.ddna() {
            ui.add_space(8.0);
            theme::section_label(ui, p, "02 / Dataset archiviati (iMazing DDNABackup)");
            ui.horizontal_wrapped(|ui| {
                for (code, desc) in ddna.flags.active_datasets() {
                    theme::chip(ui, code, p.accent).on_hover_text(desc);
                }
            });
            if ddna.photos_disk_usage > 0 || ddna.apps_disk_usage > 0 || ddna.audio_disk_usage > 0 {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ddna.photos_disk_usage > 0 {
                        ui.weak(format!("Foto: {}", fmt_size(ddna.photos_disk_usage)));
                    }
                    if ddna.apps_disk_usage > 0 {
                        ui.weak(format!(" · App: {}", fmt_size(ddna.apps_disk_usage)));
                    }
                    if ddna.audio_disk_usage > 0 {
                        ui.weak(format!(" · Audio: {}", fmt_size(ddna.audio_disk_usage)));
                    }
                    if ddna.video_disk_usage > 0 {
                        ui.weak(format!(" · Video: {}", fmt_size(ddna.video_disk_usage)));
                    }
                });
            }
        }

        if let Some(vdb) = imancraft_core::imazing_versions::find_versions_db(&b.dir) {
            ui.add_space(8.0);
            theme::section_label(ui, p, "03 / Cronologia Snapshot iMazing (Versions.db)");
            if let Ok(conn) = imancraft_core::imazing_versions::open_versions_db(&vdb) {
                let snaps = imancraft_core::imazing_versions::list_snapshots(&conn).unwrap_or_default();
                let props = imancraft_core::imazing_versions::load_properties(&conn).ok();
                ui.horizontal(|ui| {
                    theme::chip(ui, "Snapshot iMazing rilevati", p.success);
                    ui.weak(format!("Trovati {} snapshot · UDID: {}", snaps.len(), props.as_ref().map(|pr| pr.device_udid.as_str()).unwrap_or("-")));
                });
                if !snaps.is_empty() {
                    egui::CollapsingHeader::new(RichText::new(format!("📅 Cronologia Snapshot ({})", snaps.len())).strong()).default_open(true).show(ui, |ui| {
                        egui_extras::TableBuilder::new(ui)
                            .striped(true)
                            .column(egui_extras::Column::exact(45.0))
                            .column(egui_extras::Column::initial(160.0))
                            .column(egui_extras::Column::initial(140.0))
                            .column(egui_extras::Column::initial(100.0))
                            .column(egui_extras::Column::remainder())
                            .header(20.0, |mut h| {
                                h.col(|ui| { ui.weak(RichText::new("ID").monospace().size(10.0)); });
                                h.col(|ui| { ui.weak(RichText::new("DATA").monospace().size(10.0)); });
                                h.col(|ui| { ui.weak(RichText::new("MOTIVO").monospace().size(10.0)); });
                                h.col(|ui| { ui.weak(RichText::new("VERSIONE").monospace().size(10.0)); });
                                h.col(|ui| { ui.weak(RichText::new("DIMENSIONE").monospace().size(10.0)); });
                            })
                            .body(|body| {
                                body.rows(24.0, snaps.len(), |mut row| {
                                    let s = &snaps[row.index()];
                                    row.col(|ui| { ui.label(format!("#{}", s.id)); });
                                    row.col(|ui| { ui.label(super::fmt_date(s.timestamp as f64)); });
                                    row.col(|ui| { ui.weak(format!("{:?}", s.backup_reason)); });
                                    row.col(|ui| { ui.weak(&s.ios_version); });
                                    row.col(|ui| { ui.label(fmt_size(s.original_size)); });
                                });
                            });
                    });
                }
            } else {
                ui.horizontal(|ui| {
                    theme::chip(ui, "Snapshot iMazing rilevati", p.success);
                    ui.weak(format!("Database: {}", vdb.display()));
                });
            }
        }

        ui.add_space(8.0);
        theme::section_label(ui, p, "03 / Dettagli del backup");
        ui.weak(b.dir.display().to_string());

        for (title, rows) in &st.info.as_ref().unwrap().1 {
            egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(title == "Dispositivo").show(ui, |ui| {
                egui::Grid::new(title).striped(true).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (k, v) in rows {
                        ui.weak(k);
                        ui.add(egui::Label::new(super::short(v, 400)).wrap());
                        ui.end_row();
                    }
                });
            });
        }
    });
}

/// Riquadro di sblocco di un backup cifrato (la password resta solo in memoria per derivare le chiavi).
pub fn unlock_box(ui: &mut egui::Ui, p: &Palette, b: &Arc<Backup>, st: &mut State) {
    theme::card(p).show(ui, |ui| {
        ui.set_max_width(460.0);
        ui.label(RichText::new("🔒 Backup cifrato").strong().size(16.0));
        ui.label("Inserisci la password impostata in iTunes, Finder o iMazing per questo backup. Le chiavi restano solo in memoria finché l'app è aperta.");
        let busy = st.unlocking.as_ref().is_some_and(|t| t.ready().is_none());
        let resp = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut st.pw).password(true).hint_text("Password del backup").desired_width(300.0));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if (ui.add_enabled_ui(!busy && !st.pw.is_empty(), |ui| theme::primary(ui, p, "Sblocca")).inner.clicked() || enter) && !st.pw.is_empty() {
                let (b, pw) = (b.clone(), std::mem::take(&mut st.pw));
                st.unlocking = Some(tasks::run(ui.ctx(), "Sblocco backup", move || b.unlock(&pw).map_err(|e| e.to_string())));
            }
            if busy {
                ui.spinner();
                ui.weak("Derivazione della chiave (qualche secondo)…");
            }
        });
        if let Some(Err(e)) = st.unlocking.as_ref().and_then(|t| t.ready()) {
            ui.colored_label(p.danger, format!("⚠ {e}"));
        }
    });
}
