//! IManCraft — app nativa (egui + wgpu) per esplorare i backup iOS, in stile Crafting Apps.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod images;
mod player;
mod tasks;
mod theme;
pub mod upscaler;
mod views;

use egui::{Frame, Margin, RichText, Stroke};
use imancraft_core::Backup;
use std::path::PathBuf;
use std::sync::Arc;
use theme::{Palette, Settings};
use views::{Env, Nav, Section, Views};

fn app_icon() -> egui::IconData {
    let width = 64u32;
    let height = 64u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let dx = (x as f32 - 31.5).abs();
            let dy = (y as f32 - 31.5).abs();
            let corner = (dx - 22.0).max(0.0).powi(2) + (dy - 22.0).max(0.0).powi(2);
            if corner > 64.0 {
                continue;
            }
            rgba[idx] = 0x1A;
            rgba[idx + 1] = 0x1A;
            rgba[idx + 2] = 0x1E;
            rgba[idx + 3] = 0xFF;

            if x >= 18 && x <= 45 && y >= 12 && y <= 51 {
                let px = (x as f32 - 31.5).abs();
                let py = (y as f32 - 31.5).abs();
                let p_corner = (px - 10.0).max(0.0).powi(2) + (py - 16.0).max(0.0).powi(2);
                if p_corner <= 9.0 {
                    rgba[idx] = 0x4C;
                    rgba[idx + 1] = 0x8D;
                    rgba[idx + 2] = 0xFF;
                    rgba[idx + 3] = 0xFF;

                    if x >= 21 && x <= 42 && y >= 17 && y <= 45 {
                        rgba[idx] = 0x0E;
                        rgba[idx + 1] = 0x0E;
                        rgba[idx + 2] = 0x12;
                    }
                    if x >= 27 && x <= 36 && y >= 47 && y <= 48 {
                        rgba[idx] = 0xFF;
                        rgba[idx + 1] = 0xFF;
                        rgba[idx + 2] = 0xFF;
                    }
                    if x >= 28 && x <= 35 && y >= 14 && y <= 15 {
                        rgba[idx] = 0x2A;
                        rgba[idx + 1] = 0x2A;
                        rgba[idx + 2] = 0x2E;
                    }
                }
            }
        }
    }
    egui::IconData { rgba, width, height }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IManCraft")
            .with_icon(app_icon())
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 620.0])
            .with_maximized(true),
        persist_window: false, // parte sempre massimizzata (lo stato salvato di una finestra troppo grande la farebbe uscire dallo schermo)
        ..Default::default()
    };
    eframe::run_native("IManCraft", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

struct App {
    settings: Settings,
    applied: Option<Settings>,
    palette: Palette,
    backups: Vec<Arc<Backup>>,
    scan: Option<tasks::Task<Vec<Arc<Backup>>>>,
    current: Option<usize>,
    section: Section,
    views: Views,
    images: images::Images,
    viewer: Option<views::viewer::Viewer>,
    about: bool,
    show_operations: bool,
}

const SETTINGS_KEY: &str = "imancraft-settings";

impl App {
    fn new(cc: &eframe::CreationContext) -> App {
        theme::install_fonts(&cc.egui_ctx);
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, SETTINGS_KEY)).unwrap_or_default();
        let mut app = App {
            palette: theme::apply(&cc.egui_ctx, &settings),
            applied: Some(settings.clone()),
            settings,
            backups: vec![],
            scan: None,
            current: None,
            section: Section::Discover,
            views: Views::default(),
            images: images::Images::new(&cc.egui_ctx),
            viewer: None,
            about: false,
            show_operations: false,
        };
        app.rescan(&cc.egui_ctx);
        app
    }

    /// Cerca i backup (cartelle standard + aggiunte dall'utente + argomenti da riga di comando).
    fn rescan(&mut self, ctx: &egui::Context) {
        let mut roots = imancraft_core::default_roots();
        roots.extend(self.settings.folders.iter().map(PathBuf::from));
        roots.extend(std::env::args().skip(1).map(PathBuf::from));
        self.scan = Some(tasks::run(ctx, "Ricerca dei backup", move || Ok(imancraft_core::discover(&roots).into_iter().map(|d| Arc::new(Backup::open(d))).collect())));
    }

    fn select(&mut self, i: usize) {
        if self.current != Some(i) {
            self.current = Some(i);
            self.views = Views::default();
            self.viewer = None;
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Panel::top("menu")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(12, 6)).stroke(Stroke::new(1.0, p.line)))
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    theme::logo(ui, &p, 17.0);
                    ui.add_space(14.0);
                    ui.menu_button("File", |ui| {
                        if ui.button("📂 Aggiungi cartella di backup…  Ctrl+O").clicked() {
                            if let Some(dir) = rfd::FileDialog::new().set_title("Cartella che contiene i backup").pick_folder() {
                                self.settings.folders.push(dir.display().to_string());
                                self.rescan(ui.ctx());
                            }
                        }
                        if ui.button("🔄 Aggiorna elenco backup  Ctrl+R").clicked() {
                            self.rescan(ui.ctx());
                        }
                        ui.separator();
                        if ui.button("Esci").clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Visualizza", |ui| {
                        for s in Section::ALL {
                            let shortcut = match s {
                                Section::Discover => "  Ctrl+D",
                                Section::Search => "  Ctrl+F",
                                Section::Extractor => "  Ctrl+E",
                                _ => "",
                            };
                            if ui.button(format!("{}  {}{shortcut}", s.icon(), s.label())).clicked() {
                                self.section = s;
                            }
                        }
                        ui.separator();
                        if ui.button("📋 Cronologia Operazioni  Ctrl+H").clicked() {
                            self.show_operations = true;
                        }
                    });
                    ui.menu_button("Aiuto", |ui| {
                        if ui.button("Informazioni su IManCraft").clicked() {
                            self.about = true;
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dark = p.dark;
                        if ui.button(if dark { "☀" } else { "☾" }).on_hover_text("Tema chiaro/scuro").clicked() {
                            self.settings.theme = if dark { theme::ThemeMode::Light } else { theme::ThemeMode::Dark };
                        }
                        let mut c = egui::Color32::from_rgb(self.settings.accent[0], self.settings.accent[1], self.settings.accent[2]);
                        if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).on_hover_text("Colore d'accento").changed() {
                            self.settings.accent = [c.r(), c.g(), c.b()];
                        }
                        ui.add_space(8.0);
                        // selettore del backup attivo
                        let name = self.current.and_then(|i| self.backups.get(i)).map(|b| format!("📱 {} · {}", b.summary.name, b.summary.model)).unwrap_or_else(|| "Nessun backup".into());
                        let mut pick = None;
                        egui::ComboBox::from_id_salt("backup-pick").selected_text(name).width(300.0).show_ui(ui, |ui| {
                            for (i, b) in self.backups.iter().enumerate() {
                                let lock = if b.encrypted { if b.is_locked() { " 🔒" } else { " 🔓" } } else { "" };
                                if ui.selectable_label(self.current == Some(i), format!("{} · {} · iOS {}{lock}", b.summary.name, b.summary.model, b.summary.ios)).clicked() {
                                    pick = Some(i);
                                }
                            }
                        });
                        if let Some(i) = pick {
                            self.select(i);
                        }
                    });
                });
            });
    }

    fn rail(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Panel::left("rail")
            .exact_size(86.0)
            .resizable(false)
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(6, 10)).stroke(Stroke::new(1.0, p.line)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for s in Section::ALL {
                        let sel = self.section == s;
                        let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), 50.0), egui::Sense::click());
                        let painter = ui.painter();
                        if sel {
                            painter.rect_filled(rect, 4.0, p.accent.gamma_multiply(0.16));
                            painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::Vec2::new(3.0, rect.height())), 1.0, p.accent);
                        } else if resp.hovered() {
                            painter.rect_filled(rect, 4.0, p.panel_alt);
                        }
                        let color = if sel { p.accent } else { p.text };
                        painter.text(rect.center() - egui::Vec2::new(0.0, 8.0), egui::Align2::CENTER_CENTER, s.icon(), egui::FontId::proportional(19.0), color);
                        painter.text(rect.center() + egui::Vec2::new(0.0, 14.0), egui::Align2::CENTER_CENTER, s.label(), egui::FontId::proportional(10.5), if sel { p.accent } else { p.muted });
                        if resp.clicked() {
                            self.section = s;
                        }
                    }
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Panel::bottom("status")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(12, 4)).stroke(Stroke::new(1.0, p.line)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match self.current.and_then(|i| self.backups.get(i)) {
                        Some(b) => {
                            ui.label(RichText::new(format!("{} · {} · iOS {}", b.summary.name, b.summary.model, b.summary.ios)).small());
                            if b.encrypted {
                                theme::chip(ui, if b.is_locked() { "cifrato" } else { "sbloccato" }, if b.is_locked() { p.danger } else { p.success });
                            }
                        }
                        None => {
                            ui.weak(RichText::new("Nessun backup selezionato").small());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::chip(ui, if p.dark { "scuro" } else { "chiaro" }, p.muted);
                        let ops = tasks::running();
                        let hist = tasks::history();
                        let op_text = if !ops.is_empty() {
                            format!("⏳ {} in corso ({})", ops.len(), hist.len())
                        } else if !hist.is_empty() {
                            format!("📋 {} operazioni", hist.len())
                        } else {
                            "📋 Operazioni".to_string()
                        };
                        if ui.small_button(op_text).on_hover_text("Cronologia Operazioni (Ctrl+H)").clicked() {
                            self.show_operations = !self.show_operations;
                        }
                        if !ops.is_empty() {
                            ui.spinner();
                        }
                    });
                });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Scorciatoie da tastiera globali
        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::F) {
                self.section = Section::Search;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::D) {
                self.section = Section::Discover;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::E) {
                self.section = Section::Extractor;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::O) {
                if let Some(dir) = rfd::FileDialog::new().set_title("Cartella che contiene i backup").pick_folder() {
                    self.settings.folders.push(dir.display().to_string());
                    self.rescan(&ctx);
                }
            }
            if i.modifiers.command && i.key_pressed(egui::Key::R) {
                self.rescan(&ctx);
            }
            if i.modifiers.command && i.key_pressed(egui::Key::H) {
                self.show_operations = !self.show_operations;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::Comma) {
                self.section = Section::Settings;
            }
        });

        // tema/accento cambiati (o tema di sistema)
        if self.applied.as_ref() != Some(&self.settings) || self.settings.theme == theme::ThemeMode::System {
            self.palette = theme::apply(&ctx, &self.settings);
            self.applied = Some(self.settings.clone());
        }
        self.images.begin_frame();
        if let Some(Ok(list)) = self.scan.as_ref().and_then(|t| t.ready()) {
            let keep = self.current.and_then(|i| self.backups.get(i)).map(|b| b.dir.clone());
            self.backups = list.clone();
            self.current = keep.and_then(|d| self.backups.iter().position(|b| b.dir == d)).or(if self.backups.is_empty() { None } else { Some(0) });
            self.scan = None;
        }

        self.menu_bar(ui);
        self.status_bar(ui);
        self.rail(ui);
        let p = self.palette;
        let mut nav = None;
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg).inner_margin(Margin::same(14))).show(ui, |ui| {
            if self.section == Section::Settings {
                views::settings::ui(ui, &p, &mut self.settings);
                return;
            }
            if self.section == Section::Devices {
                let mut pick = None;
                views::devices::ui(ui, &p, &self.backups, self.current, self.scan.is_some(), &mut self.views.devices, &mut pick);
                if let Some(i) = pick {
                    self.select(i);
                    self.section = Section::Discover;
                }
                return;
            }
            let Some(b) = self.current.and_then(|i| self.backups.get(i)).cloned() else {
                ui.weak("Nessun backup: aggiungi una cartella dal menu File o collega un backup nelle cartelle standard.");
                return;
            };
            if b.is_locked() {
                views::devices::unlock_box(ui, &p, &b, &mut self.views.devices);
                return;
            }
            let mut env = Env { backup: &b, p: &p, images: &self.images, viewer: &mut self.viewer, nav: &mut nav };
            self.views.show(self.section, ui, &mut env);
        });
        match nav {
            Some(Nav::Files { domain, path }) => {
                self.section = Section::Files;
                self.views.files.goto(domain, path);
            }
            Some(Nav::Section(s)) => {
                self.section = s;
            }
            None => {}
        }
        if let Some(v) = &mut self.viewer {
            if !v.ui(&ctx, &p, &self.images) {
                self.viewer = None;
            }
        }
        if self.about {
            egui::Window::new("Informazioni").collapsible(false).resizable(false).open(&mut self.about).show(&ctx, |ui| {
                theme::logo(ui, &p, 22.0);
                ui.label(format!("Versione {}", env!("CARGO_PKG_VERSION")));
                ui.label("Viewer dei backup iOS in Rust (egui + wgpu). I dati restano in locale; i backup sono aperti in sola lettura.");
            });
        }
        if self.show_operations {
            egui::Window::new("📋 Cronologia Operazioni")
                .collapsible(false)
                .default_size([560.0, 360.0])
                .resizable(true)
                .open(&mut self.show_operations)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Attività ed esportazioni eseguite").size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑 Svuota cronologia").clicked() {
                                tasks::clear_history();
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.separator();

                    let running = tasks::running();
                    if !running.is_empty() {
                        ui.label(RichText::new("Operazioni in corso:").strong().color(p.accent));
                        for op in &running {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(op).strong());
                            });
                        }
                        ui.add_space(6.0);
                        ui.separator();
                    }

                    let hist = tasks::history();
                    if hist.is_empty() && running.is_empty() {
                        ui.add_space(12.0);
                        ui.weak("Nessuna operazione registrata finora.");
                    } else if !hist.is_empty() {
                        ui.label(RichText::new(format!("Completate ({})", hist.len())).strong());
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("history_grid").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                                ui.weak("Esito");
                                ui.weak("Operazione");
                                ui.weak("Durata");
                                ui.end_row();

                                for entry in hist.iter().rev() {
                                    if entry.success {
                                        ui.label(RichText::new("✓").strong().color(p.success));
                                    } else {
                                        ui.label(RichText::new("✕").strong().color(p.danger));
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(&entry.label);
                                        if let Some(err) = &entry.error {
                                            ui.label(RichText::new(err).color(p.danger).size(11.0));
                                        }
                                    });
                                    ui.label(format!("{:.2} s", entry.duration_ms as f64 / 1000.0));
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
