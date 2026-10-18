//! Schermate dell'app. Ogni vista ha il suo `State` (caricamenti in background, filtri, selezione).

pub mod apps;
pub mod devices;
pub mod discover;
pub mod files;
pub mod lists;
pub mod messages;
pub mod photos;
pub mod settings;
pub mod viewer;
pub mod whatsapp;

use crate::images::{self, Images, Img, Src};
use crate::theme::{self, Palette};
use egui::{CornerRadius, Frame, Margin, Sense, Vec2};
use imancraft_core::Backup;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Discover,
    Devices,
    Search,
    Apps,
    Files,
    Messages,
    WhatsApp,
    Telegram,
    Photos,
    Contacts,
    Notes,
    Calendars,
    Reminders,
    Calls,
    Voicemail,
    Safari,
    Spyware,
    Extractor,
    Settings,
}

impl Section {
    pub const ALL: [Section; 19] = [
        Section::Discover,
        Section::Devices,
        Section::Search,
        Section::Apps,
        Section::Files,
        Section::Messages,
        Section::WhatsApp,
        Section::Telegram,
        Section::Photos,
        Section::Contacts,
        Section::Notes,
        Section::Calendars,
        Section::Reminders,
        Section::Calls,
        Section::Voicemail,
        Section::Safari,
        Section::Spyware,
        Section::Extractor,
        Section::Settings,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Section::Discover => "🧭",
            Section::Devices => "📱",
            Section::Search => "🔎",
            Section::Apps => "▦",
            Section::Files => "📁",
            Section::Messages => "💬",
            Section::WhatsApp => "🗨",
            Section::Telegram => "✈",
            Section::Photos => "🖼",
            Section::Contacts => "👤",
            Section::Notes => "📝",
            Section::Calendars => "📅",
            Section::Reminders => "☑",
            Section::Calls => "📞",
            Section::Voicemail => "🎙",
            Section::Safari => "🌐",
            Section::Spyware => "🛡",
            Section::Extractor => "📦",
            Section::Settings => "⚙",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Section::Discover => "Discover",
            Section::Devices => "Dispositivi",
            Section::Search => "Cerca",
            Section::Apps => "App",
            Section::Files => "File",
            Section::Messages => "Messaggi",
            Section::WhatsApp => "WhatsApp",
            Section::Telegram => "Telegram",
            Section::Photos => "Foto",
            Section::Contacts => "Contatti",
            Section::Notes => "Note",
            Section::Calendars => "Calendari",
            Section::Reminders => "Promemoria",
            Section::Calls => "Chiamate",
            Section::Voicemail => "Segreteria",
            Section::Safari => "Safari",
            Section::Spyware => "Spyware",
            Section::Extractor => "Esporta Dati",
            Section::Settings => "Impostazioni",
        }
    }
}

/// Richieste di navigazione tra viste.
pub enum Nav {
    Files { domain: String, path: String },
    Section(Section),
}

/// Ciò che una vista riceve ad ogni frame.
pub struct Env<'a> {
    pub backup: &'a Arc<Backup>,
    pub p: &'a Palette,
    pub images: &'a Images,
    pub viewer: &'a mut Option<viewer::Viewer>,
    pub nav: &'a mut Option<Nav>,
}

#[derive(Default)]
pub struct Views {
    pub discover: discover::State,
    pub devices: devices::State,
    pub apps: apps::State,
    pub files: files::State,
    pub messages: messages::State,
    pub whatsapp: whatsapp::State,
    pub photos: photos::State,
    pub lists: lists::State,
}

impl Views {
    pub fn show(&mut self, s: Section, ui: &mut egui::Ui, env: &mut Env) {
        match s {
            Section::Discover => self.discover.ui(ui, env),
            Section::Apps => self.apps.ui(ui, env),
            Section::Files => self.files.ui(ui, env),
            Section::Messages => self.messages.ui(ui, env),
            Section::WhatsApp => self.whatsapp.ui(ui, env),
            Section::Telegram => self.lists.telegram(ui, env),
            Section::Photos => self.photos.ui(ui, env),
            Section::Contacts => self.lists.contacts(ui, env),
            Section::Notes => self.lists.notes(ui, env),
            Section::Calendars => self.lists.calendars(ui, env),
            Section::Reminders => self.lists.reminders(ui, env),
            Section::Calls => self.lists.calls(ui, env),
            Section::Voicemail => self.lists.voicemail(ui, env),
            Section::Safari => self.lists.safari(ui, env),
            Section::Spyware => self.lists.spyware(ui, env),
            Section::Extractor => self.lists.extractor(ui, env),
            Section::Search => self.lists.search(ui, env),
            Section::Devices | Section::Settings => {}
        }
    }
}


// ---------------------------------------------------------------- helper comuni

/// Data locale "gg/mm/aaaa hh:mm" (vuota se assente).
pub fn fmt_date(t: f64) -> String {
    // 0 = assente; WhatsApp usa anche date sentinella nel futuro (es. anno 4001)
    if t <= 0.0 || t > 4_102_444_800.0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(t as i64, 0).map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default()
}

pub fn fmt_size(n: u64) -> String {
    let mut v = n as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", units[i]) }
}

/// Fuso orario locale nella convenzione di `whatsapp::fmt_date` (minuti, UTC − locale).
pub fn tz_offset_min() -> i64 {
    -(chrono::Local::now().offset().local_minus_utc() as i64 / 60)
}

/// Intestazione di vista: etichetta "NN / SEZIONE" + titolo.
pub fn header(ui: &mut egui::Ui, p: &Palette, n: &str, section: &str, title: &str) {
    theme::section_label(ui, p, &format!("{n} / {section}"));
    theme::title(ui, title);
    ui.add_space(4.0);
}

/// Dialogo nativo "Salva con nome".
pub fn save_dialog(name: &str) -> Option<PathBuf> {
    let safe: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    rfd::FileDialog::new().set_file_name(safe).save_file()
}

/// Esporta (in background) il file `id` del backup nel percorso scelto dall'utente.
pub fn export_file(ctx: &egui::Context, b: &Arc<Backup>, id: &str, name: &str) -> Option<crate::tasks::Task<()>> {
    let dest = save_dialog(name)?;
    let id = id.to_string();
    Some(crate::tasks::load(ctx, format!("Esportazione {name}"), b, move |cx| cx.content(&id)?.save_to(&dest)))
}

/// Disegna una miniatura (immagine del backup) di lato `side`; restituisce la risposta al clic.
pub fn thumb(ui: &mut egui::Ui, env: &Env, id: &str, name: &str, side: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    paint_thumb(ui, env, rect, id, name);
    resp
}

/// Disegna una miniatura con supporto opzionale a un fallback (es. se thumb fallisce o non c'è, prova il file originale).
pub fn paint_thumb_opt(ui: &egui::Ui, env: &Env, rect: egui::Rect, id: &str, name: &str, fb_id: Option<&str>, fb_name: Option<&str>) {
    let max = (rect.width().max(rect.height()) * ui.ctx().pixels_per_point()).clamp(64.0, 512.0) as u32 / 64 * 64;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, env.p.panel_alt);
    let primary = env.images.get(&images::key(env.backup, id, max), max, || Src::File { backup: env.backup.clone(), id: id.into(), name: name.into() });
    match primary {
        Img::Ready(t) => {
            let s = t.size_vec2();
            let (ar, rr) = (s.x / s.y, rect.width() / rect.height());
            let uv = if ar > rr {
                let w = rr / ar;
                egui::Rect::from_min_max(egui::pos2((1.0 - w) / 2.0, 0.0), egui::pos2((1.0 + w) / 2.0, 1.0))
            } else {
                let h = ar / rr;
                egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - h) / 2.0), egui::pos2(1.0, (1.0 + h) / 2.0))
            };
            painter.image(t.id(), rect, uv, egui::Color32::WHITE);
        }
        Img::Loading => {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(14.0), env.p.muted);
        }
        Img::Failed(_) => {
            if let (Some(fid), Some(fname)) = (fb_id, fb_name) {
                if fid != id {
                    let fallback = env.images.get(&images::key(env.backup, fid, max), max, || Src::File { backup: env.backup.clone(), id: fid.into(), name: fname.into() });
                    match fallback {
                        Img::Ready(t) => {
                            let s = t.size_vec2();
                            let (ar, rr) = (s.x / s.y, rect.width() / rect.height());
                            let uv = if ar > rr {
                                let w = rr / ar;
                                egui::Rect::from_min_max(egui::pos2((1.0 - w) / 2.0, 0.0), egui::pos2((1.0 + w) / 2.0, 1.0))
                            } else {
                                let h = ar / rr;
                                egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - h) / 2.0), egui::pos2(1.0, (1.0 + h) / 2.0))
                            };
                            painter.image(t.id(), rect, uv, egui::Color32::WHITE);
                            return;
                        }
                        Img::Loading => {
                            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(14.0), env.p.muted);
                            return;
                        }
                        Img::Failed(_) => {}
                    }
                }
            }
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, short(name, 14), egui::FontId::proportional(10.0), env.p.muted);
        }
    }
}

/// Disegna l'immagine `id` riempiendo `rect` (ritaglio centrato), con segnaposto durante il caricamento.
pub fn paint_thumb(ui: &egui::Ui, env: &Env, rect: egui::Rect, id: &str, name: &str) {
    paint_thumb_opt(ui, env, rect, id, name, None, None);
}

/// Badge in un angolo della miniatura.
pub fn badge(ui: &egui::Ui, rect: egui::Rect, text: &str, bottom_left: bool) {
    let font = egui::FontId::proportional(10.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
    let size = galley.size() + Vec2::new(8.0, 2.0);
    let min = if bottom_left { egui::pos2(rect.min.x + 4.0, rect.max.y - size.y - 4.0) } else { egui::pos2(rect.max.x - size.x - 4.0, rect.min.y + 4.0) };
    let r = egui::Rect::from_min_size(min, size);
    ui.painter().rect_filled(r, 8.0, egui::Color32::from_black_alpha(170));
    ui.painter().galley(r.min + Vec2::new(4.0, 1.0), galley, egui::Color32::WHITE);
}

pub fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

pub fn bubble(ui: &mut egui::Ui, p: &Palette, me: bool, _id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let right_pad = 10.0;
    let avail_w = ui.available_width();
    let max_w = ((avail_w - right_pad) * 0.72).clamp(160.0, (avail_w - right_pad - 10.0).max(160.0));
    let margin_x = 20.0;
    let content_max_w = (max_w - margin_x).max(40.0);

    if me {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(right_pad);
            Frame::new()
                .fill(p.accent)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_max_width(content_max_w);
                        ui.visuals_mut().override_text_color = Some(p.on_accent);
                        ui.visuals_mut().hyperlink_color = p.on_accent;
                        add(ui);
                    });
                })
                .response
        })
        .inner
    } else {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            Frame::new()
                .fill(p.bubble)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(content_max_w);
                    ui.vertical(add);
                })
                .response
        })
        .inner
    }
}

/// Pannello laterale sinistro con margine verso il contenuto.
pub fn side_panel(id: &str, p: &Palette) -> egui::Panel {
    egui::Panel::left(id.to_owned())
        .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 0, right: 14, top: 0, bottom: 0 }))
        .default_size(320.0)
        .size_range(220.0..=540.0)
}

/// Filtro testuale standard per le liste.
pub fn filter_box(ui: &mut egui::Ui, text: &mut String, hint: &str) -> bool {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(260.0)).changed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn test_bubble_alignment() {
        let ctx = egui::Context::default();
        let p = Palette::new(true, [255, 120, 0]);

        for _frame in 1..=4 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                ui.set_width(1280.0);
                egui::Panel::left("rail").exact_size(86.0).show(ui, |_ui| {});
                egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin::same(14))).show(ui, |ui| {
                    super::side_panel("wa-chats", &p).show(ui, |_ui| {});
                    let avail_w = ui.available_width();
                    let cursor_min = ui.cursor().min;
                    let right_bound = cursor_min.x + avail_w;

                    egui::ScrollArea::vertical().id_salt("wa-msgs").auto_shrink(false).show(ui, |ui| {
                        // Messaggio breve inviato
                        let short_resp = bubble(ui, &p, true, "short_1", |ui| {
                            ui.label("Ok grazie!");
                        });
                        assert!(short_resp.rect.max.x <= right_bound, "Short bubble exceeded available area: {} > {}", short_resp.rect.max.x, right_bound);
                        let dist_from_right = right_bound - short_resp.rect.max.x;
                        assert!((dist_from_right - 10.0).abs() < 1.5, "Short bubble not aligned to right: dist={dist_from_right}, expected ~10.0");

                        // Messaggio lungo inviato con wrapping
                        let long_resp = bubble(ui, &p, true, "long_1", |ui| {
                            ui.add(egui::Label::new("Questo è un messaggio inviato molto lungo che contiene molte parole per verificare come egui calcola la larghezza e se per caso va a finire oltre il bordo destro dello schermo quando non c'è wrap esplicito!").selectable(true).wrap());
                        });
                        assert!(long_resp.rect.max.x <= right_bound, "Long bubble exceeded available area: {} > {}", long_resp.rect.max.x, right_bound);
                        let dist_from_right = right_bound - long_resp.rect.max.x;
                        assert!((dist_from_right - 10.0).abs() < 1.5, "Long bubble not aligned to right: dist={dist_from_right}, expected ~10.0");

                        // Messaggio ricevuto (a sinistra)
                        let recv_resp = bubble(ui, &p, false, "recv_1", |ui| {
                            ui.label("Ciao, ti andrebbe di vederci?");
                        });
                        assert!(recv_resp.rect.min.x >= cursor_min.x, "Received bubble out of left bound: {} < {}", recv_resp.rect.min.x, cursor_min.x);
                        assert!(recv_resp.rect.max.x <= right_bound, "Received bubble exceeded right bound: {} > {}", recv_resp.rect.max.x, right_bound);
                    });
                });
            });
            out.textures_delta.clear();
        }
    }
}



