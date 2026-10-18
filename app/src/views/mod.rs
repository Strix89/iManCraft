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

/// Disegna l'immagine `id` riempiendo `rect` (ritaglio centrato), con segnaposto durante il caricamento.
pub fn paint_thumb(ui: &egui::Ui, env: &Env, rect: egui::Rect, id: &str, name: &str) {
    let max = (rect.width().max(rect.height()) * ui.ctx().pixels_per_point()).clamp(64.0, 512.0) as u32 / 64 * 64;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, env.p.panel_alt);
    match env.images.get(&images::key(env.backup, id, max), max, || Src::File { backup: env.backup.clone(), id: id.into(), name: name.into() }) {
        Img::Ready(t) => {
            let s = t.size_vec2();
            // ritaglio centrato tipo "cover"
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
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, short(name, 14), egui::FontId::proportional(10.0), env.p.muted);
        }
    }
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

/// Bolla di chat (a destra se `me`).
/// Le bolle "mie" si allineano a destra usando la larghezza misurata al frame precedente (egui non conosce la
/// larghezza del contenuto prima di disegnarlo; un layout allineato a destra la allargherebbe a tutta riga).
pub fn bubble(ui: &mut egui::Ui, p: &Palette, me: bool, id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let right_pad = 10.0;
    let max_w = ((ui.available_width() - right_pad) * 0.72).max(220.0);
    let key = ui.make_persistent_id(("bubble", id));
    let prev: Option<f32> = ui.data(|d| d.get_temp(key));
    ui.horizontal(|ui| {
        if me {
            ui.add_space((ui.available_width() - prev.unwrap_or(max_w) - right_pad).max(0.0));
        }
        let r = Frame::new()
            .fill(if me { p.accent } else { p.bubble })
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_max_width(max_w);
                if me {
                    ui.visuals_mut().override_text_color = Some(p.on_accent);
                    ui.visuals_mut().hyperlink_color = p.on_accent;
                }
                ui.vertical(add);
            })
            .response;
        if me {
            ui.add_space(right_pad);
        }
        if me && prev.is_none_or(|w| (w - r.rect.width()).abs() > 0.5) {
            ui.data_mut(|d| d.insert_temp(key, r.rect.width()));
            ui.ctx().request_repaint();
        }
        r
    })
    .inner
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
