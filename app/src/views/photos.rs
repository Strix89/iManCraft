//! Foto: intera libreria Photos.sqlite (anche elementi solo su iCloud con anteprima), filtri e album; griglia virtualizzata.

use super::{badge, header, paint_thumb, viewer, Env};
use crate::player::fmt_time;
use crate::tasks::{self, Task};
use egui::{Sense, Vec2};
use imancraft_core::data::{self, Album, Photo};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum F {
    #[default]
    All,
    Local,
    Cloud,
    Photo,
    Video,
    Fav,
    Hidden,
    Trashed,
}

const FILTERS: [(F, &str); 8] = [
    (F::All, "Tutti"),
    (F::Local, "Sul dispositivo"),
    (F::Cloud, "Solo iCloud ☁"),
    (F::Photo, "Foto"),
    (F::Video, "Video"),
    (F::Fav, "Preferiti ♥"),
    (F::Hidden, "Nascosti"),
    (F::Trashed, "Eliminati di recente"),
];

fn keep(f: F, p: &Photo) -> bool {
    let live = !p.trashed;
    match f {
        F::All => live && !p.hidden,
        F::Local => live && !p.cloud,
        F::Cloud => live && p.cloud,
        F::Photo => live && !p.video,
        F::Video => live && p.video,
        F::Fav => live && p.fav,
        F::Hidden => live && p.hidden,
        F::Trashed => p.trashed,
    }
}

pub struct State {
    data: Option<Task<(Vec<Photo>, Vec<Album>)>>,
    filter: F,
    album: Option<usize>,
    side: f32,
    counts: Option<Vec<usize>>,
    export_job: Option<Task<usize>>,
}

impl Default for State {
    fn default() -> Self {
        State { data: None, filter: F::All, album: None, side: 132.0, counts: None, export_job: None }
    }
}

impl State {
    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        header(ui, env.p, "05", "Libreria foto", "Foto");
        if self.data.is_none() {
            self.data = Some(tasks::load(ui.ctx(), "Libreria foto", env.backup, data::photos));
        }
        let Some((items, albums)) = tasks::show(ui, self.data.as_ref()) else { return };
        let counts = self.counts.get_or_insert_with(|| FILTERS.iter().map(|(f, _)| items.iter().filter(|p| keep(*f, p)).count()).collect());
        ui.horizontal_wrapped(|ui| {
            for ((f, label), n) in FILTERS.iter().zip(counts.iter()) {
                if ui.selectable_label(self.filter == *f, format!("{label}  {n}")).clicked() {
                    self.filter = *f;
                }
            }
            if !albums.is_empty() {
                let text = self.album.and_then(|i| albums.get(i)).map_or("Tutti gli album".into(), |a| format!("{} ({})", a.title, a.pks.len()));
                egui::ComboBox::from_id_salt("album").selected_text(text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.album, None, "Tutti gli album");
                    for (i, a) in albums.iter().enumerate() {
                        ui.selectable_value(&mut self.album, Some(i), format!("{} ({})", a.title, a.pks.len()));
                    }
                });
            }
            ui.add(egui::Slider::new(&mut self.side, 80.0..=260.0).show_value(false).text("dimensione"));
        });
        let set: Option<std::collections::HashSet<i64>> = self.album.and_then(|i| albums.get(i)).map(|a| a.pks.iter().copied().collect());
        let list: Vec<&Photo> = items.iter().filter(|p| keep(self.filter, p) && set.as_ref().is_none_or(|s| s.contains(&p.pk))).collect();
        let nc = list.iter().filter(|p| p.cloud).count();
        ui.horizontal(|ui| {
            ui.weak(format!(
                "{} elementi{} · clic per aprire, ← → per scorrere",
                list.len(),
                if nc > 0 { format!(" · {nc} solo su iCloud (nel backup c'è l'anteprima)") } else { String::new() }
            ));
            let busy = self.export_job.as_ref().is_some_and(|t| t.ready().is_none());
            if ui.add_enabled(!busy && !list.is_empty(), egui::Button::new(format!("⬇ Esporta selezione ({} foto)…", list.len()))).clicked() {
                if let Some(dest) = rfd::FileDialog::new().set_title("Cartella destinazione foto").pick_folder() {
                    let to_export: Vec<(Option<String>, Option<String>, String)> = list
                        .iter()
                        .map(|p| (p.id.clone(), p.thumb.clone(), p.name.clone()))
                        .collect();
                    self.export_job = Some(tasks::load(ui.ctx(), "Esportazione foto", env.backup, move |cx| {
                        let mut count = 0;
                        for (idx, (id, thumb, name)) in to_export.iter().enumerate() {
                            let target_id = id.as_ref().or(thumb.as_ref());
                            if let Some(fid) = target_id {
                                let ext = if name.contains('.') { name.rsplit('.').next().unwrap_or("jpg") } else { "jpg" };
                                let filename = format!("Foto_{:04}_{}.{}", idx + 1, fid.get(..8).unwrap_or(""), ext);
                                if let Ok(content) = cx.content(fid) {
                                    let _ = content.save_to(&dest.join(filename));
                                    count += 1;
                                }
                            }
                        }
                        Ok(count)
                    }));
                }
            }
            if busy {
                ui.spinner();
                ui.weak("Esportazione foto in corso…");
            }
            if let Some(Ok(n)) = self.export_job.as_ref().and_then(|t| t.ready()) {
                ui.colored_label(env.p.success, format!("✓ Esportate {n} foto"));
            }
        });
        ui.add_space(4.0);

        let gap = 6.0;
        let cols = ((ui.available_width() + gap) / (self.side + gap)).floor().max(1.0) as usize;
        let rows = list.len().div_ceil(cols);
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, self.side + gap, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (k, p) in list.iter().enumerate().skip(r * cols).take(cols) {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(self.side), Sense::click());
                        // video sul dispositivo: primo frame (ffmpeg); foto e iCloud: miniatura iOS
                        match (&p.id, &p.thumb) {
                            (Some(id), _) if p.video => paint_thumb(ui, env, rect, id, &p.name),
                            (_, Some(th)) => paint_thumb(ui, env, rect, th, "thumb.jpg"),
                            (Some(id), None) => paint_thumb(ui, env, rect, id, &p.name),
                            _ => {}
                        }
                        if p.video {
                            badge(ui, rect, &format!("▶ {}", fmt_time(p.duration)), true);
                        }
                        if p.cloud {
                            badge(ui, rect, "☁", false);
                        } else if p.fav {
                            badge(ui, rect, "♥", false);
                        }
                        if resp.hovered() {
                            ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(2.0, env.p.accent), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            open = Some(k);
                        }
                    }
                });
                ui.add_space(gap - ui.spacing().item_spacing.y);
            }
        });
        if let Some(k) = open {
            let items = list
                .iter()
                .map(|p| viewer::Item {
                    backup: env.backup.clone(),
                    id: p.id.clone(),
                    thumb: p.thumb.clone(),
                    name: p.name.clone(),
                    t: p.t,
                    size: p.size,
                    video: p.video,
                    cloud: p.cloud,
                    w: p.w,
                    h: p.h,
                })
                .collect();
            *env.viewer = Some(viewer::Viewer::new(items, k));
        }
    }
}
