//! Design system "Craft" (ispirato alle Crafting Apps di ArtCraft): antracite denso, bordi sottili, un colore d'accento
//! personalizzabile, etichette monospazio maiuscole con spaziatura larga, tema chiaro/scuro.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Stroke, TextFormat, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub theme: ThemeMode,
    /// colore d'accento sRGB
    pub accent: [u8; 3],
    pub compact: bool,
    /// fattore di zoom dell'interfaccia
    pub zoom: f32,
    /// cartelle di backup aggiunte dall'utente (oltre a quelle standard)
    pub folders: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: ThemeMode::Dark, accent: ACCENTS[0].1, compact: true, zoom: 1.0, folders: vec![] }
    }
}

/// Accenti predefiniti (una tinta per app, come nella suite ArtCraft).
pub const ACCENTS: [(&str, [u8; 3]); 7] = [
    ("Blu", [0x4C, 0x8D, 0xFF]),
    ("Arancio", [0xFF, 0x7A, 0x3D]),
    ("Viola", [0x9B, 0x6B, 0xFF]),
    ("Ambra", [0xFF, 0xB0, 0x20]),
    ("Teal", [0x14, 0xB8, 0xA6]),
    ("Magenta", [0xE8, 0x45, 0x9B]),
    ("Verde", [0x3C, 0xCB, 0x7F]),
];

/// Colori risolti per il tema corrente.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub bubble: Color32,
}

impl Palette {
    pub fn new(dark: bool, accent: [u8; 3]) -> Palette {
        let accent = Color32::from_rgb(accent[0], accent[1], accent[2]);
        // testo sull'accento: nero o bianco secondo la luminanza
        let lum = 0.2126 * accent.r() as f32 + 0.7152 * accent.g() as f32 + 0.0722 * accent.b() as f32;
        let on_accent = if lum > 150.0 { Color32::from_rgb(0x11, 0x11, 0x14) } else { Color32::WHITE };
        let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
        if dark {
            Palette {
                dark,
                bg: hex(0x0E0E10),
                panel: hex(0x151518),
                panel_alt: hex(0x1C1C21),
                line: hex(0x2A2A31),
                text: hex(0xE9E9EE),
                muted: hex(0x8C8C99),
                accent,
                on_accent,
                danger: hex(0xFF5C5C),
                success: hex(0x3CCB7F),
                bubble: hex(0x232329),
            }
        } else {
            Palette {
                dark,
                bg: hex(0xF4F4F6),
                panel: hex(0xFFFFFF),
                panel_alt: hex(0xEDEDF0),
                line: hex(0xDCDCE2),
                text: hex(0x121215),
                muted: hex(0x6B6B76),
                accent,
                on_accent,
                danger: hex(0xD93636),
                success: hex(0x1F9D5C),
                bubble: hex(0xE7E7EC),
            }
        }
    }
}

/// Font: Segoe UI / Cascadia (se presenti in Windows) davanti ai font di egui, che restano come fallback (emoji).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let mut add = |name: &str, files: &[&str], family: FontFamily, front: bool| {
        if let Some(bytes) = files.iter().find_map(|f| std::fs::read(format!(r"{windir}\Fonts\{f}")).ok()) {
            fonts.font_data.insert(name.into(), FontData::from_owned(bytes).into());
            let list = fonts.families.entry(family).or_default();
            if front {
                list.insert(0, name.into());
            } else {
                list.push(name.into());
            }
        }
    };
    add("ui", &["segoeui.ttf"], FontFamily::Proportional, true);
    add("mono", &["CascadiaMono.ttf", "consola.ttf"], FontFamily::Monospace, true);
    add("ui-bold", &["seguisb.ttf", "segoeuib.ttf"], FontFamily::Name("bold".into()), true);
    add("symbols", &["seguisym.ttf"], FontFamily::Proportional, false);
    // la famiglia "bold" ricade sui font proporzionali se Segoe manca
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.entry(FontFamily::Name("bold".into())).or_default().extend(fallback);
    ctx.set_fonts(fonts);
}

/// Applica tema, accento e densità a tutti gli stili di egui.
pub fn apply(ctx: &egui::Context, s: &Settings) -> Palette {
    let dark = match s.theme {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => ctx.system_theme().map_or(true, |t| t == egui::Theme::Dark),
    };
    let p = Palette::new(dark, s.accent);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(3);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(4);
    v.extreme_bg_color = if dark { Color32::from_rgb(0x0A, 0x0A, 0x0C) } else { Color32::WHITE };
    v.faint_bg_color = p.panel_alt;
    v.code_bg_color = p.panel_alt;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.muted);
    v.error_fg_color = p.danger;
    v.striped = true;
    v.window_shadow.color = Color32::from_black_alpha(if dark { 90 } else { 40 });
    v.popup_shadow.color = Color32::from_black_alpha(if dark { 90 } else { 40 });
    let w = &mut v.widgets;
    for (wv, fill, stroke) in [
        (&mut w.noninteractive, p.panel, p.line),
        (&mut w.inactive, p.panel_alt, p.line),
        (&mut w.hovered, p.panel_alt.gamma_multiply(1.0), p.accent.gamma_multiply(0.7)),
        (&mut w.active, p.accent.gamma_multiply(0.8), p.accent),
        (&mut w.open, p.panel_alt, p.accent),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = Stroke::new(1.0, stroke);
        wv.corner_radius = r;
        wv.expansion = 0.0;
    }
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    // (il testo "strong" di egui usa questo colore: deve restare leggibile anche nel tema chiaro)
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    ctx.all_styles_mut(|st| {
        st.visuals = v.clone();
        let sp = &mut st.spacing;
        sp.item_spacing = if s.compact { Vec2::new(6.0, 4.0) } else { Vec2::new(8.0, 7.0) };
        sp.button_padding = if s.compact { Vec2::new(8.0, 3.0) } else { Vec2::new(11.0, 6.0) };
        sp.interact_size.y = if s.compact { 22.0 } else { 28.0 };
        sp.window_margin = Margin::same(10);
        sp.menu_margin = Margin::same(6);
        st.interaction.selectable_labels = true;
    });
    ctx.set_zoom_factor(s.zoom.clamp(0.7, 2.0));
    p
}

/// Etichetta di sezione stile Crafting Apps: "01 / DISPOSITIVI" monospazio, maiuscolo, spaziatura larga.
pub fn section_label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat { font_id: FontId::monospace(10.5), color: p.muted, extra_letter_spacing: 1.6, ..Default::default() },
    );
    ui.label(job);
}

/// Chip monospazio con bordo e testo colorati ("WINDOWS", "CIFRATO", "ICLOUD").
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.8)))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            let mut job = egui::text::LayoutJob::default();
            job.append(&text.to_uppercase(), 0.0, TextFormat { font_id: FontId::monospace(9.5), color, extra_letter_spacing: 1.0, ..Default::default() });
            ui.label(job)
        })
        .response
}

/// Logo testuale: "IMan" + "Craft" nel colore d'accento.
pub fn logo(ui: &mut egui::Ui, p: &Palette, size: f32) {
    let bold = FontId::new(size, FontFamily::Name("bold".into()));
    let mut job = egui::text::LayoutJob::default();
    job.append("IMan", 0.0, TextFormat { font_id: bold.clone(), color: p.text, ..Default::default() });
    job.append("Craft", 0.0, TextFormat { font_id: bold, color: p.accent, ..Default::default() });
    ui.label(job);
}

/// Pulsante primario pieno nel colore d'accento.
pub fn primary(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(p.on_accent)).fill(p.accent).stroke(Stroke::new(1.0, p.accent)))
}

/// Titolo grande di una vista.
pub fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(FontId::new(19.0, FontFamily::Name("bold".into()))));
}

/// Riquadro "card" con bordo sottile.
pub fn card(p: &Palette) -> egui::Frame {
    egui::Frame::new().fill(p.panel_alt).stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_on_accent() {
        assert_eq!(Palette::new(true, [0xFF, 0xB0, 0x20]).on_accent, Color32::from_rgb(0x11, 0x11, 0x14)); // ambra → testo scuro
        assert_eq!(Palette::new(true, [0x4C, 0x8D, 0xFF]).on_accent, Color32::WHITE); // blu → testo bianco
    }
}
