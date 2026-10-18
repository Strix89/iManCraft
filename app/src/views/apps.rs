//! App installate: icona, nome, sviluppatore, versione, numero di file; clic → file dell'app.

use super::{filter_box, header, Env, Nav};
use crate::images::{Img, Src};
use crate::tasks::{self, Task};
use crate::theme;
use egui::{RichText, Sense, Vec2};
use imancraft_core::data::{self, App};
use std::sync::Arc;

#[derive(Default)]
pub struct State {
    list: Option<Task<Vec<App>>>,
    filter: String,
    export_job: Option<Task<usize>>,
}

impl State {
    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "02", "Applicazioni", "App");
        if self.list.is_none() {
            self.list = Some(tasks::load(ui.ctx(), "Elenco app", env.backup, data::apps));
        }
        let Some(apps) = tasks::show(ui, self.list.as_ref()) else { return };
        filter_box(ui, &mut self.filter, &format!("Cerca tra {} app…", apps.len()));
        ui.add_space(6.0);

        if let Some(job) = &self.export_job {
            match job.ready() {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.weak("Esportazione container .imazingapp in corso…");
                    });
                }
                Some(Ok(cnt)) => {
                    ui.colored_label(env.p.success, format!("✅ Container .imazingapp esportato con successo ({cnt} file inclusi)"));
                }
                Some(Err(e)) => {
                    ui.colored_label(env.p.danger, format!("Errore esportazione: {e}"));
                }
            }
            ui.add_space(4.0);
        }

        let f = self.filter.to_lowercase();
        let mut export_req = None;

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for a in apps.iter().filter(|a| f.is_empty() || format!("{} {} {}", a.name, a.bundle, a.artist).to_lowercase().contains(&f)) {
                    let mut open_files = false;
                    theme::card(env.p).show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.horizontal(|ui| {
                            let (rect, icon_resp) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::click());
                            match (&a.icon, a.icon.as_ref().map(|b| env.images.get(&format!("icon:{}", a.bundle), 96, || Src::Bytes(Arc::from(b.as_slice()))))) {
                                (_, Some(Img::Ready(t))) => {
                                    ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                                }
                                _ => {
                                    ui.painter().rect_filled(rect, 10.0, env.p.line);
                                }
                            }
                            if icon_resp.clicked() {
                                open_files = true;
                            }
                            ui.vertical(|ui| {
                                if ui.link(RichText::new(super::short(&a.name, 26)).strong()).clicked() {
                                    open_files = true;
                                }
                                ui.weak(super::short(if a.artist.is_empty() { &a.bundle } else { &a.artist }, 30));
                                ui.weak(format!("{}{} file", if a.version.is_empty() { String::new() } else { format!("v{} · ", a.version) }, a.files));
                            });
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            if ui.button("📦 Esporta .imazingapp").on_hover_text("Esporta i dati sandbox in formato container iMazing").clicked() {
                                let safe_name: String = a.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
                                if let Some(dest) = super::save_dialog(&format!("{}.imazingapp", safe_name.trim())) {
                                    export_req = Some((a.bundle.clone(), dest));
                                }
                            }
                        });
                    });
                    if open_files {
                        *env.nav = Some(Nav::Files { domain: format!("AppDomain-{}", a.bundle), path: String::new() });
                    }
                }
            });
        });

        if let Some((bundle_id, dest)) = export_req {
            self.export_job = Some(tasks::load(ui.ctx(), "Esportazione .imazingapp", env.backup, move |cx| {
                imancraft_core::imazing_app::export_imazing_app(cx, &bundle_id, &dest)
            }));
        }
    }
}

