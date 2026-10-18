//! iMessage / SMS: conversazioni e messaggi con allegati.

use super::{bubble, export_file, filter_box, fmt_date, header, thumb, viewer, Env};
use crate::tasks::{self, Task};
use egui::RichText;
use imancraft_core::data::{self, Chat, Message};

#[derive(Default)]
pub struct State {
    chats: Option<Task<Vec<Chat>>>,
    filter: String,
    sel: Option<i64>,
    sel_title: String,
    msgs: Option<Task<Vec<Message>>>,
    export: Option<Task<()>>,
}

impl State {
    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        if self.chats.is_none() {
            self.chats = Some(tasks::load(ui.ctx(), "Conversazioni", env.backup, data::chats));
        }
        super::side_panel("sms-list", env.p).show(ui, |ui| {
            header(ui, env.p, "04", "iMessage e SMS", "Messaggi");
            let Some(chats) = tasks::show(ui, self.chats.as_ref()) else { return };
            filter_box(ui, &mut self.filter, &format!("Cerca tra {} conversazioni…", chats.len()));
            let f = self.filter.to_lowercase();
            let list: Vec<&Chat> = chats.iter().filter(|c| f.is_empty() || c.title.to_lowercase().contains(&f)).collect();
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 44.0, list.len(), |ui, range| {
                for c in &list[range] {
                    let r = ui.selectable_label(self.sel == Some(c.id), RichText::new(super::short(&c.title, 38)).strong());
                    ui.weak(format!("{} · {} msg · {}", c.service, c.count, fmt_date(c.last)));
                    if r.clicked() && self.sel != Some(c.id) {
                        self.sel = Some(c.id);
                        self.sel_title = c.title.clone();
                        let id = c.id;
                        self.msgs = Some(tasks::load(ui.ctx(), "Messaggi", env.backup, move |cx| data::chat(cx, id)));
                    }
                }
            });
        });
        let Some(msgs) = tasks::show(ui, self.msgs.as_ref()) else {
            if self.sel.is_none() {
                ui.weak("Seleziona una conversazione.");
            }
            return;
        };
        if let Some(Err(e)) = self.export.as_ref().and_then(|t| t.ready()) {
            ui.colored_label(env.p.danger, format!("⚠ {e}"));
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.sel_title).strong().size(15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⬇ HTML (Stampa/PDF)").on_hover_text("Esporta la conversazione in formato HTML pronto da aprire e stampare in PDF").clicked() {
                    let title = if self.sel_title.is_empty() { "Messaggi".to_string() } else { self.sel_title.clone() };
                    if let Some(dest) = super::save_dialog(&format!("iMessage - {title}.html")) {
                        let msgs_clone = msgs.clone();
                        self.export = Some(tasks::run(ui.ctx(), "Esportazione messaggi HTML", move || {
                            imancraft_core::export::export_messages_html(&title, &msgs_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
                if ui.button("⬇ XLSX (Excel)").on_hover_text("Esporta la conversazione in formato Excel").clicked() {
                    let title = if self.sel_title.is_empty() { "Messaggi".to_string() } else { self.sel_title.clone() };
                    if let Some(dest) = super::save_dialog(&format!("iMessage - {title}.xlsx")) {
                        let msgs_clone = msgs.clone();
                        self.export = Some(tasks::run(ui.ctx(), "Esportazione messaggi Excel", move || {
                            imancraft_core::export::export_messages_xlsx(&title, &msgs_clone, &dest).map_err(|e| e.to_string())
                        }));
                    }
                }
            });
        });
        ui.separator();

        // tutti i media della conversazione, per sfogliarli nel visualizzatore
        let media: Vec<viewer::Item> = msgs
            .iter()
            .flat_map(|m| m.att.iter().map(move |a| (m, a)))
            .filter_map(|(m, a)| {
                let id = a.id.as_ref()?;
                let vis = a.mime.starts_with("image/") || a.mime.starts_with("video/") || imancraft_core::is_image(&a.name) || imancraft_core::is_video(&a.name);
                vis.then(|| viewer::Item { video: a.mime.starts_with("video/") || imancraft_core::is_video(&a.name), ..viewer::Item::file(env.backup, id, &a.name, m.t, 0) })
            })
            .collect();
        let p = *env.p;
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            let mut day = String::new();
            for (i, m) in msgs.iter().enumerate() {
                let d = fmt_date(m.t).get(..10).unwrap_or("").to_string();
                if d != day {
                    ui.vertical_centered(|ui| ui.weak(RichText::new(if d.is_empty() { "data sconosciuta" } else { &d }).small()));
                    day = d;
                }
                ui.add_space(2.0);
                bubble(ui, &p, m.me, i, |ui| {
                    if !m.me {
                        ui.label(RichText::new(&m.from).small().strong().color(p.accent));
                    }
                    if !m.text.is_empty() {
                        ui.add(egui::Label::new(&m.text).selectable(true));
                    }
                    for a in &m.att {
                        let Some(id) = &a.id else {
                            ui.weak(format!("📎 {} (non nel backup)", a.name));
                            continue;
                        };
                        if let Some(k) = media.iter().position(|x| x.id.as_deref() == Some(id.as_str())) {
                            if thumb(ui, env, id, &a.name, 180.0).on_hover_text(&a.name).clicked() {
                                *env.viewer = Some(viewer::Viewer::new(media.clone(), k));
                            }
                        } else if ui.link(format!("📎 {}", a.name)).on_hover_text("Esporta…").clicked() {
                            self.export = export_file(ui.ctx(), env.backup, id, &a.name).or(self.export.take());
                        }
                    }
                    ui.label(RichText::new(fmt_date(m.t).get(11..).unwrap_or("").to_string()).small().weak());
                });
            }
        });
    }
}
