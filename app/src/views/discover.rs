//! Schermata Discover (ispirata ad iMazing 3):
//! Panoramica del dispositivo, barra ripartizione memoria per categoria e azioni rapide.

use super::{fmt_size, header, Env, Nav, Section};
use crate::theme;
use egui::{Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Vec2};

#[derive(Default)]
pub struct State {}

impl State {
    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        let b = env.backup;
        let p = env.p;

        header(ui, p, "01", "Panoramica e azioni rapide", "Discover");

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // 1. Hero Card del Dispositivo
            theme::card(p).show(ui, |ui| {
                ui.set_width(ui.available_width() - 8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if b.summary.model.to_lowercase().contains("ipad") { "📟" } else { "📱" }).size(44.0));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&b.summary.name).strong().size(20.0));
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&b.summary.model).strong());
                            ui.weak("·");
                            ui.weak(format!("iOS {}", b.summary.ios));
                            ui.weak("·");
                            ui.weak(format!("Backup del {}", b.summary.date.get(..10).unwrap_or(&b.summary.date)));
                            if b.summary.size > 0 {
                                ui.weak("·");
                                ui.weak(format!("Spazio: {}", fmt_size(b.summary.size)));
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if b.encrypted {
                                theme::chip(ui, if b.is_locked() { "cifrato" } else { "sbloccato" }, if b.is_locked() { p.danger } else { p.success });
                            } else {
                                theme::chip(ui, "non cifrato", p.muted);
                            }
                            if let Some(ddna) = b.ddna() {
                                theme::chip(ui, "iMazing Backup", p.accent).on_hover_text("Metadati DDNABackup.plist presenti");
                                theme::chip(ui, &format!("{} dataset attivi", ddna.flags.active_datasets().len()), p.accent);
                            }
                            if imancraft_core::imazing_versions::find_versions_db(&b.dir).is_some() {
                                theme::chip(ui, "Snapshot Versions.db", p.success);
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // 2. Storage Bar (Ripartizione Spazio stile iMazing 3)
            if let Some(ddna) = b.ddna() {
                theme::section_label(ui, p, "Archiviazione e quote disco");
                ui.add_space(4.0);

                let total = if ddna.total_disk_usage > 0 {
                    ddna.total_disk_usage as f64
                } else if b.summary.size > 0 {
                    b.summary.size as f64
                } else {
                    1.0
                };

                let p_photos = ddna.photos_disk_usage as f64;
                let p_apps = ddna.apps_disk_usage as f64;
                let p_audio = ddna.audio_disk_usage as f64;
                let p_video = ddna.video_disk_usage as f64;
                let p_other = (total - p_photos - p_apps - p_audio - p_video).max(0.0);

                let col_photos = Color32::from_rgb(0xE8, 0x45, 0x9B); // Magenta
                let col_apps = Color32::from_rgb(0x4C, 0x8D, 0xFF);   // Blu
                let col_audio = Color32::from_rgb(0x9B, 0x6B, 0xFF);  // Viola
                let col_video = Color32::from_rgb(0x14, 0xB8, 0xA6);  // Teal
                let col_other = p.muted;

                // Barra segmentata
                let bar_height = 14.0;
                let bar_w = ui.available_width() - 8.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_w, bar_height), Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, p.panel_alt);

                let mut current_x = rect.min.x;
                let draw_segment = |current_x: &mut f32, val: f64, color: Color32| {
                    if val <= 0.0 {
                        return;
                    }
                    let seg_w = (val / total * rect.width() as f64) as f32;
                    let seg_rect = Rect::from_min_size(egui::pos2(*current_x, rect.min.y), Vec2::new(seg_w, bar_height));
                    painter.rect_filled(seg_rect, 0.0, color);
                    *current_x += seg_w;
                };

                draw_segment(&mut current_x, p_photos, col_photos);
                draw_segment(&mut current_x, p_apps, col_apps);
                draw_segment(&mut current_x, p_audio, col_audio);
                draw_segment(&mut current_x, p_video, col_video);
                draw_segment(&mut current_x, p_other, col_other);

                ui.add_space(4.0);

                // Legenda
                ui.horizontal_wrapped(|ui| {
                    let legend = |ui: &mut egui::Ui, name: &str, val: u64, color: Color32| {
                        if val > 0 {
                            ui.horizontal(|ui| {
                                let (c_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                ui.painter_at(c_rect).circle_filled(c_rect.center(), 4.5, color);
                                ui.label(RichText::new(format!("{name}: {}", fmt_size(val))).size(12.0));
                            });
                            ui.add_space(10.0);
                        }
                    };
                    legend(ui, "Foto", ddna.photos_disk_usage, col_photos);
                    legend(ui, "App", ddna.apps_disk_usage, col_apps);
                    legend(ui, "Audio", ddna.audio_disk_usage, col_audio);
                    legend(ui, "Video", ddna.video_disk_usage, col_video);
                    if p_other > 0.0 {
                        legend(ui, "Altro / Dati", p_other as u64, col_other);
                    }
                });

                ui.add_space(14.0);
            }

            // 3. Griglia Azioni Rapide (Stile Quick Actions di iMazing 3)
            theme::section_label(ui, p, "Azioni rapide");
            ui.add_space(6.0);

            let actions: [(&str, &str, &str, Section); 8] = [
                ("📦", "Backup Extractor", "Esporta tutti i dati del backup in cartelle ordinate", Section::Extractor),
                ("🔎", "Ricerca Globale", "Cerca simultaneamente parole chiave in tutto il backup", Section::Search),
                ("💬", "Messaggi & SMS", "Conversazioni a bolle ed export Excel/HTML", Section::Messages),
                ("🗨", "WhatsApp", "Chat complete con ricerca data/ora, vocali ed export", Section::WhatsApp),
                ("🖼", "Foto & Video", "Sfoglia la galleria multimediale ed esporta in blocco", Section::Photos),
                ("👤", "Contatti & Rubrica", "Elenco contatti ed export vCard (.vcf) o Excel", Section::Contacts),
                ("☑", "Promemoria", "Attività, scadenze, elenchi e priorità", Section::Reminders),
                ("🛡", "Rilevamento Spyware", "Scansione offline indicatori Pegasus e Predator", Section::Spyware),
            ];

            let avail_w = ui.available_width() - 8.0;
            let cols = if avail_w >= 840.0 { 4 } else if avail_w >= 440.0 { 2 } else { 1 };
            let spacing = 10.0;
            let card_w = ((avail_w - (cols as f32 - 1.0) * spacing) / cols as f32).max(180.0);
            let card_h = 76.0;

            egui::Grid::new("discover_actions_grid")
                .num_columns(cols)
                .spacing([spacing, spacing])
                .show(ui, |ui| {
                    for (idx, (icon, title, desc, target_sec)) in actions.iter().enumerate() {
                        let r = Frame::new()
                            .fill(p.panel)
                            .corner_radius(CornerRadius::same(6))
                            .stroke(Stroke::new(1.0, p.line))
                            .inner_margin(Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.set_width(card_w);
                                ui.set_height(card_h);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(*icon).size(26.0));
                                    ui.add_space(6.0);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(*title).strong().size(13.5));
                                        ui.weak(RichText::new(*desc).size(11.0));
                                    });
                                });
                            })
                            .response
                            .interact(Sense::click());

                        if r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            *env.nav = Some(Nav::Section(*target_sec));
                        }
                        if (idx + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(14.0);

            // 4. Sezione Dettagli Tecnici Dispositivo
            theme::section_label(ui, p, "Informazioni dispositivo");
            ui.add_space(4.0);
            theme::card(p).show(ui, |ui| {
                ui.set_width(ui.available_width() - 8.0);
                egui::Grid::new("discover_device_details")
                    .num_columns(2)
                    .spacing([20.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.weak("Identificativo UDID:");
                        ui.label(RichText::new(&b.summary.udid).monospace().size(11.5));
                        ui.end_row();

                        ui.weak("Percorso backup:");
                        ui.label(RichText::new(b.dir.display().to_string()).monospace().size(11.5));
                        ui.end_row();

                        ui.weak("Stato crittografia:");
                        ui.label(if b.encrypted { "Cifrato (chiavi protette in memoria)" } else { "Non protetto da password" });
                        ui.end_row();

                        if let Some(ddna) = b.ddna() {
                            if !ddna.apple_id.is_empty() {
                                ui.weak("Apple ID associato:");
                                ui.label(&ddna.apple_id);
                                ui.end_row();
                            }
                            if !ddna.model_details.is_empty() {
                                ui.weak("Dettagli modello:");
                                ui.label(&ddna.model_details);
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }
}
