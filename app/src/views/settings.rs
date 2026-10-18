//! Impostazioni: tema, colore d'accento, densità, zoom, cartelle di backup.

use super::header;
use crate::theme::{self, Palette, Settings, ThemeMode, ACCENTS};
use egui::{Color32, RichText, Sense, Stroke, Vec2};

pub fn ui(ui: &mut egui::Ui, p: &Palette, s: &mut Settings) {
    header(ui, p, "99", "Impostazioni", "Impostazioni");
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        theme::section_label(ui, p, "Aspetto");
        theme::card(p).show(ui, |ui| {
            ui.set_max_width(620.0);
            ui.horizontal(|ui| {
                ui.label("Tema");
                ui.selectable_value(&mut s.theme, ThemeMode::Dark, "☾ Scuro");
                ui.selectable_value(&mut s.theme, ThemeMode::Light, "☀ Chiaro");
                ui.selectable_value(&mut s.theme, ThemeMode::System, "Sistema");
            });
            ui.add_space(6.0);
            ui.label("Colore d'accento");
            ui.horizontal_wrapped(|ui| {
                for (name, rgb) in ACCENTS {
                    let c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    let (rect, r) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::click());
                    ui.painter().rect_filled(rect.shrink(3.0), 4.0, c);
                    if s.accent == rgb {
                        ui.painter().rect_stroke(rect, 5.0, Stroke::new(2.0, p.text), egui::StrokeKind::Inside);
                    }
                    if r.on_hover_text(name).clicked() {
                        s.accent = rgb;
                    }
                }
                let mut c = Color32::from_rgb(s.accent[0], s.accent[1], s.accent[2]);
                ui.add_space(6.0);
                ui.label("Personalizzato");
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    s.accent = [c.r(), c.g(), c.b()];
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.compact, "Layout compatto");
                ui.add_space(16.0);
                ui.label("Zoom");
                ui.add(egui::Slider::new(&mut s.zoom, 0.8..=1.6).step_by(0.05).fixed_decimals(2));
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.weak("Anteprima:");
                theme::logo(ui, p, 16.0);
                theme::chip(ui, "chip", p.accent);
                let _ = ui.button("Pulsante");
                ui.label(RichText::new("link").color(p.accent));
            });
        });

        ui.add_space(12.0);
        theme::section_label(ui, p, "Cartelle di backup");
        theme::card(p).show(ui, |ui| {
            ui.set_max_width(620.0);
            ui.weak("Oltre alle cartelle standard di iTunes/Apple Devices. Si aggiungono da File → Aggiungi cartella di backup…");
            let mut remove = None;
            for (i, f) in s.folders.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.monospace(f);
                    if ui.small_button("✕").on_hover_text("Rimuovi (al prossimo aggiornamento dell'elenco)").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.folders.remove(i);
            }
        });
    });
}
