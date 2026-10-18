//! File system del backup: domini → cartelle → file; anteprima media/plist/testo, esportazione.

use super::{export_file, fmt_date, fmt_size, header, viewer, Env};
use crate::tasks::{self, Task};
use egui::RichText;
use egui_extras::{Column, TableBuilder};
use imancraft_core::data::{self, Domain, Entry};

#[derive(Default)]
pub struct State {
    domains: Option<Task<Vec<Domain>>>,
    filter: String,
    domain: Option<String>,
    path: String,
    entries: Option<Task<Vec<Entry>>>,
    loaded_for: Option<(String, String)>,
    text: Option<(String, Task<String>)>,
    export: Option<Task<()>>,
}

impl State {
    /// Apre un dominio/cartella (es. da "App").
    pub fn goto(&mut self, domain: String, path: String) {
        self.domain = Some(domain);
        self.path = path;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        if self.domains.is_none() {
            self.domains = Some(tasks::load(ui.ctx(), "Domini", env.backup, data::domains));
        }
        super::side_panel("files-domains", env.p).show(ui, |ui| {
            header(ui, env.p, "03", "File system", "Domini");
            super::filter_box(ui, &mut self.filter, "Filtra domini…");
            let Some(domains) = tasks::show(ui, self.domains.as_ref()) else { return };
            let f = self.filter.to_lowercase();
            let list: Vec<&Domain> = domains.iter().filter(|d| f.is_empty() || d.name.to_lowercase().contains(&f)).collect();
            let mut pick = None;
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 26.0, list.len(), |ui, range| {
                for d in &list[range] {
                    let is_sel = self.domain.as_deref() == Some(d.name.as_str());
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
                    if is_sel {
                        ui.painter().rect_filled(rect, 3.0, env.p.accent.gamma_multiply(0.18));
                        ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, env.p.accent), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 3.0, env.p.panel_alt);
                    }
                    let pt = ui.painter_at(rect);
                    let label = format!("{}  ({})", super::short(&d.name, 44), d.count);
                    pt.text(
                        egui::pos2(rect.min.x + 6.0, rect.min.y + 4.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(12.0),
                        if is_sel { env.p.text } else { env.p.muted },
                    );
                    if resp.on_hover_text(&d.name).clicked() {
                        pick = Some(d.name.clone());
                    }
                }
            });
            if let Some(d) = pick {
                self.domain = Some(d);
                self.path.clear();
            }
        });

        let Some(domain) = self.domain.clone() else {
            ui.weak("Seleziona un dominio a sinistra.");
            return;
        };
        let want = (domain.clone(), self.path.clone());
        if self.loaded_for.as_ref() != Some(&want) {
            let (d, path) = want.clone();
            self.entries = Some(tasks::load(ui.ctx(), "Contenuto cartella", env.backup, move |cx| data::ls(cx, &d, &path)));
            self.loaded_for = Some(want);
        }
        // percorso cliccabile
        ui.horizontal_wrapped(|ui| {
            if ui.link(RichText::new(&domain).strong()).clicked() {
                self.path.clear();
            }
            let parts: Vec<String> = self.path.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            for (i, part) in parts.iter().enumerate() {
                ui.weak("/");
                if ui.link(part).clicked() {
                    self.path = parts[..=i].join("/");
                }
            }
        });
        if let Some(Err(e)) = self.export.as_ref().and_then(|t| t.ready()) {
            ui.colored_label(env.p.danger, format!("⚠ Esportazione: {e}"));
        }
        let Some(entries) = tasks::show(ui, self.entries.as_ref()) else { return };
        if entries.is_empty() {
            ui.weak("Cartella vuota.");
            return;
        }
        let media: Vec<&Entry> = entries.iter().filter(|e| !e.dir && (imancraft_core::is_image(&e.name) || imancraft_core::is_video(&e.name))).collect();
        let mut open_dir = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::remainder().at_least(260.0))
            .column(Column::exact(90.0))
            .column(Column::exact(130.0))
            .column(Column::exact(150.0))
            .header(22.0, |mut h| {
                for t in ["NOME", "DIMENSIONE", "MODIFICATO", ""] {
                    h.col(|ui| {
                        ui.weak(RichText::new(t).monospace().size(10.5));
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, entries.len(), |mut row| {
                    let e = &entries[row.index()];
                    row.col(|ui| {
                        if e.dir {
                            if ui.link(format!("📁 {}", e.name)).clicked() {
                                open_dir = Some(e.path.clone());
                            }
                        } else {
                            let icon = if imancraft_core::is_video(&e.name) {
                                "🎬"
                            } else if imancraft_core::is_image(&e.name) {
                                "🖼"
                            } else if imancraft_core::is_audio(&e.name) {
                                "🎵"
                            } else {
                                "📄"
                            };
                            ui.label(format!("{icon} {}", e.name));
                        }
                    });
                    row.col(|ui| {
                        if !e.dir {
                            ui.weak(fmt_size(e.size));
                        }
                    });
                    row.col(|ui| {
                        ui.weak(fmt_date(e.mtime as f64));
                    });
                    row.col(|ui| {
                        if e.dir {
                            return;
                        }
                        if let Some(k) = media.iter().position(|m| m.id == e.id) {
                            if ui.small_button("Apri").clicked() {
                                let items = media.iter().map(|m| viewer::Item::file(env.backup, &m.id, &m.name, m.mtime as f64, m.size)).collect();
                                *env.viewer = Some(viewer::Viewer::new(items, k));
                            }
                        } else if is_texty(&e.name) {
                            if ui.small_button("Vedi").clicked() {
                                let (id, plist) = (e.id.clone(), e.name.to_lowercase().ends_with(".plist"));
                                let task = tasks::load(ui.ctx(), "Lettura file", env.backup, move |cx| {
                                    if plist {
                                        return data::plist_xml(cx, &id);
                                    }
                                    let c = cx.content(&id)?;
                                    let mut v = vec![];
                                    std::io::Read::read_to_end(&mut c.reader(0, c.size.min(2 << 20))?, &mut v)?;
                                    Ok(String::from_utf8_lossy(&v).into_owned())
                                });
                                self.text = Some((e.name.clone(), task));
                            }
                        }
                        if ui.small_button("Esporta…").clicked() {
                            self.export = export_file(ui.ctx(), env.backup, &e.id, &e.name).or(self.export.take());
                        }
                    });
                });
            });
        if let Some(p) = open_dir {
            self.path = p;
        }
        // finestra di testo/plist
        let mut close = false;
        if let Some((name, task)) = &self.text {
            let mut open = true;
            egui::Window::new(name.as_str()).open(&mut open).default_size([720.0, 520.0]).show(ui.ctx(), |ui| {
                if let Some(t) = tasks::show(ui, Some(task)) {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(t).monospace()).selectable(true));
                    });
                }
            });
            close = !open;
        }
        if close {
            self.text = None;
        }
    }
}

fn is_texty(name: &str) -> bool {
    matches!(name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(), "plist" | "txt" | "log" | "json" | "xml" | "csv" | "vcf" | "ics" | "html" | "js" | "strings" | "md" | "ini" | "conf")
}
