//! Visualizzatore a tutto schermo di foto e video (sovrapposto all'app).

use super::{fmt_date, fmt_size, save_dialog};
use crate::images::{self, Images, Img, Src};
use crate::player::Player;
use crate::tasks::{self, Task};
use crate::theme::{self, Palette};
use crate::upscaler::{self, GPU};
use egui::{Color32, Key, RichText, Stroke, Vec2};
use imancraft_core::{media, Backup};
use std::sync::Arc;

/// Un elemento sfogliabile nel visualizzatore.
#[derive(Clone)]
pub struct Item {
    pub backup: Arc<Backup>,
    /// originale nel backup (None = assente: si mostra `thumb`)
    pub id: Option<String>,
    pub thumb: Option<String>,
    pub name: String,
    pub t: f64,
    pub size: u64,
    pub video: bool,
    /// elemento rimasto solo su iCloud
    pub cloud: bool,
    pub w: i64,
    pub h: i64,
}

impl Item {
    pub fn file(b: &Arc<Backup>, id: &str, name: &str, t: f64, size: u64) -> Item {
        Item { backup: b.clone(), id: Some(id.into()), thumb: None, name: name.into(), t, size, video: imancraft_core::is_video(name), cloud: false, w: 0, h: 0 }
    }
}

pub struct Viewer {
    items: Vec<Item>,
    i: usize,
    player: Option<(usize, Player)>,
    zoom: f32,
    pan: Vec2,
    export: Option<Task<()>>,
    // Super-risoluzione AI
    upscaling: Option<Task<image::DynamicImage>>,
    upscaled: Option<(usize, egui::TextureHandle, u32, u32, Arc<image::DynamicImage>)>,
    show_upscaled: bool,
    upscaler_err: Option<String>,
    show_ai_modal: bool,
    force_cpu: bool,
    scale: u32,
}

impl Viewer {
    pub fn new(items: Vec<Item>, i: usize) -> Viewer {
        Viewer {
            i: i.min(items.len().saturating_sub(1)),
            items,
            player: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            export: None,
            upscaling: None,
            upscaled: None,
            show_upscaled: false,
            upscaler_err: None,
            show_ai_modal: false,
            force_cpu: false,
            scale: 4,
        }
    }

    fn step(&mut self, d: isize) {
        let n = self.items.len() as isize;
        if n > 1 {
            self.i = ((self.i as isize + d).rem_euclid(n)) as usize;
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
            self.upscaling = None;
            self.show_upscaled = false;
            self.upscaler_err = None;
        }
    }

    /// Disegna il visualizzatore; `false` = chiuso.
    pub fn ui(&mut self, ctx: &egui::Context, p: &Palette, images: &Images) -> bool {
        if self.items.is_empty() {
            return false;
        }

        // Gestione completamento super-risoluzione AI
        if let Some(task) = &self.upscaling {
            if let Some(res) = task.ready() {
                match res {
                    Ok(dyn_img) => {
                        let w = dyn_img.width();
                        let h = dyn_img.height();
                        // Per il viewport egui/wgpu, limitiamo la texture a max 4096 px
                        // per non superare max_texture_dimension_2d hardware e non saturare la VRAM
                        let display_img = if w > 4096 || h > 4096 {
                            dyn_img.thumbnail(4096, 4096)
                        } else {
                            dyn_img.clone()
                        };
                        let color_img = upscaler::to_color_image(&display_img);
                        let tex = ctx.load_texture("imancraft-upscaled", color_img, egui::TextureOptions::LINEAR);
                        self.upscaled = Some((self.i, tex, w, h, Arc::new(dyn_img.clone())));
                        self.show_upscaled = true;
                        self.upscaler_err = None;
                    }
                    Err(e) => {
                        self.upscaler_err = Some(e.clone());
                    }
                }
                self.upscaling = None;
            }
        }

        let (esc, left, right, space) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight), i.key_pressed(Key::Space)));
        if esc {
            if self.show_ai_modal {
                self.show_ai_modal = false;
                return true;
            }
            return false;
        }
        if left {
            self.step(-1);
        }
        if right {
            self.step(1);
        }
        let mut open = true;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("imancraft-viewer")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(242));
            let it = self.items[self.i].clone();
            let bar_h = 44.0;
            let stage = egui::Rect::from_min_max(screen.min + Vec2::new(0.0, bar_h), screen.max - Vec2::new(0.0, bar_h + 8.0));

            // barra superiore: nome, posizione, data, dimensione, nota
            let top = egui::Rect::from_min_size(screen.min, Vec2::new(screen.width(), bar_h));
            ui.scope_builder(egui::UiBuilder::new().max_rect(top.shrink2(Vec2::new(14.0, 8.0))), |ui| {
                ui.horizontal(|ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::from_gray(235));
                    ui.label(RichText::new(&it.name).strong());
                    ui.label(format!("{}/{}", self.i + 1, self.items.len()));
                    if it.t > 0.0 {
                        ui.label(fmt_date(it.t));
                    }
                    if it.size > 0 {
                        ui.label(fmt_size(it.size));
                    }
                    let (res_w, res_h) = if let Some((_, player)) = &self.player {
                        if player.probe.width > 0 && player.probe.height > 0 {
                            (player.probe.width as i64, player.probe.height as i64)
                        } else {
                            (it.w, it.h)
                        }
                    } else {
                        (it.w, it.h)
                    };
                    if res_w > 0 && res_h > 0 {
                        ui.label(RichText::new(format!("{}×{}", res_w, res_h)).color(Color32::from_gray(180)));
                    }
                    if it.id.is_none() {
                        let note = if it.cloud {
                            format!("☁ {} solo su iCloud: nel backup c'è solo l'anteprima{}", if it.video { "Video" } else { "Foto" }, if it.w > 0 { format!(" (originale {}×{})", it.w, it.h) } else { String::new() })
                        } else {
                            "Originale non presente nel backup: solo anteprima".into()
                        };
                        ui.colored_label(p.accent, note);
                    }
                    if let Some(err) = &self.upscaler_err {
                        ui.colored_label(p.danger, format!("⚠ Super-risoluzione: {err}"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕ Chiudi").clicked() {
                            open = false;
                        }
                        let busy = self.export.as_ref().is_some_and(|t| t.ready().is_none()) || self.upscaling.is_some();
                        if busy {
                            ui.spinner();
                        }

                        // Controlli Super-Risoluzione AI
                        let is_upscaled = self.upscaled.as_ref().map(|u| u.0) == Some(self.i);
                        if is_upscaled {
                            let (_, _, uw, uh, full_img) = self.upscaled.as_ref().unwrap();
                            let scale = self.scale;
                            let btn_label = format!("⬇ Salva {scale}× JPEG…");
                            if ui.add_enabled(!busy, egui::Button::new(btn_label)).clicked() {
                                let base = it.name.rsplit_once('.').map_or(it.name.as_str(), |x| x.0);
                                if let Some(dest) = save_dialog(&format!("{base}-AI-{scale}x.jpg")) {
                                    let full_img = full_img.clone();
                                    self.export = Some(tasks::run(ui.ctx(), "Salvataggio immagine migliorata", move || {
                                        let mut f = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
                                        full_img.write_to(&mut f, image::ImageFormat::Jpeg).map_err(|e| e.to_string())
                                    }));
                                }
                            }
                            if ui.selectable_label(self.show_upscaled, format!("✨ {}× AI ({}×{})", self.scale, uw, uh)).clicked() {
                                self.show_upscaled = true;
                            }
                            if ui.selectable_label(!self.show_upscaled, "Originale").clicked() {
                                self.show_upscaled = false;
                            }
                            if ui.add_enabled(!busy, egui::Button::new("⚙")).on_hover_text("Cambia impostazioni e scala di restauro AI").clicked() {
                                self.show_ai_modal = true;
                            }
                        } else if !it.video || it.id.is_none() {
                            let label = if it.cloud || it.id.is_none() {
                                "✨ Restaura Anteprima iCloud (AI)…"
                            } else if GPU.is_gpu_acceleration_available() {
                                "✨ Migliora con AI (GPU 4× / 8×)…"
                            } else {
                                "✨ Migliora Risoluzione (4× / 8×)…"
                            };
                            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                                self.show_ai_modal = true;
                            }
                        }

                        match (&it.id, &it.thumb) {
                            (Some(id), _) => {
                                if it.name.to_lowercase().ends_with(".heic") && ui.add_enabled(!busy, egui::Button::new("Esporta come JPEG…")).clicked() {
                                    if let Some(dest) = save_dialog(&format!("{}.jpg", it.name.rsplit_once('.').map_or(it.name.as_str(), |x| x.0))) {
                                        let (b, id) = (it.backup.clone(), id.clone());
                                        self.export = Some(tasks::run(ui.ctx(), "Conversione HEIC → JPEG", move || {
                                            let jpg = media::heic_to_jpeg(media::register(&b), &id).map_err(|e| e.to_string())?;
                                            std::fs::write(dest, jpg).map_err(|e| e.to_string())
                                        }));
                                    }
                                }
                                if ui.add_enabled(!busy, egui::Button::new("Esporta originale…")).clicked() {
                                    self.export = super::export_file(ui.ctx(), &it.backup, id, &it.name).or(self.export.take());
                                }
                            }
                            (None, Some(th)) => {
                                if ui.add_enabled(!busy, egui::Button::new("Esporta anteprima…")).clicked() {
                                    let name = format!("{}-anteprima.jpg", it.name.rsplit_once('.').map_or(it.name.as_str(), |x| x.0));
                                    self.export = super::export_file(ui.ctx(), &it.backup, th, &name).or(self.export.take());
                                }
                            }
                            _ => {}
                        }
                        if let Some(Err(e)) = self.export.as_ref().and_then(|t| t.ready()) {
                            ui.colored_label(p.danger, format!("⚠ {e}"));
                        }
                    });
                });
            });

            // contenuto
            ui.scope_builder(egui::UiBuilder::new().max_rect(stage), |ui| {
                ui.centered_and_justified(|ui| {
                    if it.video && it.id.is_some() {
                        if self.player.as_ref().map(|p| p.0) != Some(self.i) {
                            let url = media::url(media::register(&it.backup), it.id.as_deref().unwrap());
                            self.player = Some((self.i, Player::open(url)));
                        }
                        let player = &mut self.player.as_mut().unwrap().1;
                        if space {
                            player.toggle();
                        }
                        ui.vertical_centered(|ui| {
                            ui.visuals_mut().override_text_color = Some(Color32::from_gray(235));
                            player.ui(ui, stage.size() - Vec2::new(40.0, 50.0));
                        });
                        return;
                    }
                    self.player = None;

                    // Se è attiva la visualizzazione super-risolta:
                    if self.show_upscaled && self.upscaled.as_ref().map(|u| u.0) == Some(self.i) {
                        let (tex, w, h) = {
                            let (_, t, w, h, _) = self.upscaled.as_ref().unwrap();
                            (t.clone(), *w, *h)
                        };
                        self.image(ui, &tex, stage);
                        let r = egui::Rect::from_min_size(stage.right_top() - Vec2::new(260.0, -10.0), Vec2::new(250.0, 24.0));
                        ui.put(r, |ui: &mut egui::Ui| {
                            theme::chip(ui, &format!("✨ {}× SUPER-RES ATTIVA ({}×{})", self.scale, w, h), p.accent)
                        });
                        return;
                    }

                    let (id, name, max) = match (&it.id, &it.thumb) {
                        (Some(id), _) => (id.clone(), it.name.clone(), 0),
                        (None, Some(th)) => (th.clone(), "anteprima.jpg".to_string(), 0),
                        _ => return,
                    };
                    let src = || Src::File { backup: it.backup.clone(), id: id.clone(), name: name.clone() };
                    match images.get(&images::key(&it.backup, &id, max), max, src) {
                        Img::Ready(t) => self.image(ui, &t, stage),
                        Img::Loading => {
                            // Se c'è un'anteprima già pronta in cache, mostriamola subito!
                            let mut shown_thumb = false;
                            if let Some(th) = &it.thumb {
                                let th_key = images::key(&it.backup, th, 512);
                                let th_src = || Src::File { backup: it.backup.clone(), id: th.clone(), name: "t.jpg".into() };
                                if let Img::Ready(th_t) = images.get(&th_key, 512, th_src) {
                                    self.image(ui, &th_t, stage);
                                    let r = egui::Rect::from_min_size(stage.right_top() - Vec2::new(220.0, -10.0), Vec2::new(210.0, 24.0));
                                    ui.put(r, |ui: &mut egui::Ui| {
                                        theme::chip(ui, "Caricamento alta risoluzione…", p.accent)
                                    });
                                    shown_thumb = true;
                                }
                            }
                            if !shown_thumb {
                                ui.spinner();
                            }
                        }
                        Img::Failed(e) => {
                            // Fallback automatico su anteprima se l'originale non è leggibile
                            let mut shown_thumb = false;
                            if let Some(th) = &it.thumb {
                                let th_key = images::key(&it.backup, th, 512);
                                let th_src = || Src::File { backup: it.backup.clone(), id: th.clone(), name: "t.jpg".into() };
                                match images.get(&th_key, 512, th_src) {
                                    Img::Ready(th_t) => {
                                        self.image(ui, &th_t, stage);
                                        let r = egui::Rect::from_min_size(stage.right_top() - Vec2::new(260.0, -10.0), Vec2::new(250.0, 24.0));
                                        ui.put(r, |ui: &mut egui::Ui| {
                                            theme::chip(ui, "Anteprima (originale non presente)", p.muted)
                                        });
                                        shown_thumb = true;
                                    }
                                    Img::Loading => {
                                        ui.spinner();
                                        shown_thumb = true;
                                    }
                                    _ => {}
                                }
                            }
                            if !shown_thumb {
                                ui.colored_label(p.danger, format!("Anteprima non disponibile: {e}"));
                            }
                        }
                    }
                });
            });

            // Modal di configurazione e avvio restauro AI
            if self.show_ai_modal {
                let modal_w = 540.0;
                let modal_h = 440.0;
                let modal_rect = egui::Rect::from_center_size(screen.center(), Vec2::new(modal_w, modal_h));

                // Intercetta clic sullo sfondo per non trascinare l'immagine dietro
                ui.interact(screen, ui.make_persistent_id("ai-modal-bg"), egui::Sense::click());
                ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(190));
                ui.painter().rect_filled(modal_rect, 6.0, p.panel);
                ui.painter().rect_stroke(modal_rect, 6.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Outside);

                ui.scope_builder(egui::UiBuilder::new().max_rect(modal_rect.shrink(18.0)), |ui| {
                    ui.vertical(|ui| {
                        theme::section_label(ui, p, "01 / RESTAURO & SUPER-RISOLUZIONE AI LOCALE");
                        ui.add_space(4.0);
                        let header_title = if it.cloud || it.id.is_none() {
                            "✨ Restauro SOTA Anteprima iCloud (4× / 8×)"
                        } else {
                            "✨ Super-Risoluzione SOTA Locale (4× / 8×)"
                        };
                        ui.label(RichText::new(header_title).size(16.0).strong().color(p.text));
                        ui.add_space(4.0);

                        let subtitle = if it.cloud || it.id.is_none() {
                            format!("L'originale è solo su iCloud: applicazione del restauro multi-stadio per '{}'.", it.name)
                        } else {
                            format!("Pipeline SOTA unificata a 6 stadi per la massima nitidezza fotografica di '{}'.", it.name)
                        };
                        ui.label(RichText::new(subtitle).size(11.5).color(p.muted));
                        ui.add_space(8.0);

                        // Selezione Hardware & Avvisi CPU
                        let gpu_avail = GPU.is_gpu_acceleration_available();

                        ui.horizontal(|ui| {
                            if gpu_avail {
                                let gpu_label = format!("⚡ GPU ({})", if GPU.name.len() > 24 { &GPU.name[..24] } else { &GPU.name });
                                if ui.selectable_label(!self.force_cpu, gpu_label).clicked() {
                                    self.force_cpu = false;
                                }
                                if ui.selectable_label(self.force_cpu, "🐌 Processore (CPU)").clicked() {
                                    self.force_cpu = true;
                                }
                            } else {
                                theme::chip(ui, "HARDWARE: SOLO CPU", Color32::from_rgb(240, 160, 40));
                                ui.label(RichText::new("Nessuna GPU compatibile rilevata").small().color(p.muted));
                            }
                        });
                        ui.add_space(6.0);

                        let is_cpu_mode = self.force_cpu || !gpu_avail;

                        if !is_cpu_mode {
                            // Box GPU Attiva
                            egui::Frame::new()
                                .fill(p.panel_alt)
                                .stroke(Stroke::new(1.0, p.line))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        theme::chip(ui, "GPU ACCELERATA", p.success);
                                        ui.label(RichText::new("Accelerazione Hardware Vulkan 1.3 Attiva").strong().color(p.text));
                                    });
                                    ui.add_space(4.0);
                                    ui.label(RichText::new("⚡ Elaborazione tensoriale GPU ultra-rapida (~1-3s a 4×, ~5-8s a 8×).").small().color(p.muted));
                                });
                        } else {
                            // Box Avviso CPU
                            egui::Frame::new()
                                .fill(Color32::from_rgb(42, 28, 14))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(220, 140, 20)))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        theme::chip(ui, "⚠ MODALITÀ CPU", Color32::from_rgb(240, 160, 40));
                                        ui.label(RichText::new("Esecuzione su Processore (CPU)").strong().color(Color32::from_rgb(255, 215, 130)));
                                    });
                                    ui.add_space(4.0);
                                    let warn_msg = if gpu_avail && self.force_cpu {
                                        "Hai selezionato manualmente la CPU. Il calcolo su processore richiede più tempo e memoria RAM."
                                    } else {
                                        "Nessuna GPU accelerata rilevata. L'elaborazione avverrà con algoritmi sinc/CAS ottimizzati per CPU."
                                    };
                                    ui.label(RichText::new(warn_msg).small().color(Color32::from_rgb(240, 220, 180)));
                                });
                        }

                        ui.add_space(8.0);
                        theme::section_label(ui, p, "02 / PIPELINE SOTA IMMAGINE PERFETTA (6 STADI)");
                        ui.add_space(4.0);

                        egui::Frame::new()
                            .fill(p.panel_alt)
                            .stroke(Stroke::new(1.0, p.line))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                ui.label(RichText::new("✨ Ingegneria di restauro a 6 passaggi paralleli sempre attiva:").strong().small().color(p.text));
                                ui.add_space(3.0);
                                ui.label(RichText::new("1. Denoise Bilaterale Edge-Preserving  •  2. Super-Risoluzione Neurale Real-ESRGAN x4plus (Vulkan)\n3. Fedeltà Cromatica YCbCr / Wavelet  •  4. Recupero Dinamico Ombre e Mezzitoni\n5. Contrast-Adaptive Sharpening AMD CAS  •  6. Micro-Texture Fotografica Organica").small().color(p.muted));
                            });

                        ui.add_space(8.0);
                        theme::section_label(ui, p, "03 / FATTORE D'INGRANDIMENTO");
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.selectable_label(self.scale == 4, "4× (Alta Risoluzione)").clicked() {
                                self.scale = 4;
                            }
                            if ui.selectable_label(self.scale == 8, "8× (Ultra Gigapixel)").clicked() {
                                self.scale = 8;
                            }
                            if self.scale == 8 {
                                ui.label(RichText::new("⚡ Cascata Neurale 8× su GPU (4× + 2×)").small().color(p.success));
                            }
                        });

                        ui.add_space(10.0);
                        // Pulsanti azione
                        ui.horizontal(|ui| {
                            let scale = self.scale;
                            let action_label = if is_cpu_mode {
                                format!("⚡ Avvia Restauro SOTA CPU ({}×)", scale)
                            } else {
                                format!("⚡ Avvia Restauro SOTA con GPU ({}×)", scale)
                            };

                            let start_clicked = if is_cpu_mode {
                                ui.button(RichText::new(&action_label).color(Color32::from_rgb(255, 200, 100))).clicked()
                            } else {
                                theme::primary(ui, p, &action_label).clicked()
                            };

                            if start_clicked {
                                self.show_ai_modal = false;
                                let backup = it.backup.clone();
                                let it_clone = it.clone();
                                let scale = self.scale;
                                let force_cpu = self.force_cpu;
                                let task_label = if force_cpu || !GPU.is_gpu_acceleration_available() {
                                    format!("Pipeline SOTA CPU ({}×)", scale)
                                } else {
                                    format!("Pipeline SOTA Immagine Perfetta (GPU {}×)", scale)
                                };
                                self.upscaler_err = None;
                                self.upscaling = Some(tasks::run(ui.ctx(), task_label, move || {
                                    // Caricamento resiliente: prova prima l'originale (se non solo su iCloud), poi l'anteprima
                                    let img = if !it_clone.cloud {
                                        if let Some(id) = &it_clone.id {
                                            let src = Src::File { backup: backup.clone(), id: id.clone(), name: it_clone.name.clone() };
                                            images::load_dynamic_image(&src).ok()
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };

                                    let img = match img {
                                        Some(im) => im,
                                        None => {
                                            if let Some(th) = &it_clone.thumb {
                                                let src = Src::File { backup: backup.clone(), id: th.clone(), name: "anteprima.jpg".into() };
                                                images::load_dynamic_image(&src)?
                                            } else if let Some(id) = &it_clone.id {
                                                let src = Src::File { backup: backup.clone(), id: id.clone(), name: it_clone.name.clone() };
                                                images::load_dynamic_image(&src)?
                                            } else {
                                                return Err("Nessuna sorgente immagine valida nel backup".into());
                                            }
                                        }
                                    };

                                    upscaler::upscale_configured(&img, upscaler::RestorationModel::UltraEnhance, scale, &GPU, force_cpu, false)
                                }));
                            }

                            if ui.button("✕ Annulla").clicked() {
                                self.show_ai_modal = false;
                            }
                        });
                    });
                });
            }

            // Dialog Modale ArtCraft durante l'elaborazione AI
            if self.upscaling.is_some() {
                ctx.request_repaint(); // animazione fluida continua
                let modal_w = 460.0;
                let modal_h = 230.0;
                let modal_rect = egui::Rect::from_center_size(screen.center(), Vec2::new(modal_w, modal_h));

                ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(180));
                ui.painter().rect_filled(modal_rect, 6.0, p.panel);
                ui.painter().rect_stroke(modal_rect, 6.0, Stroke::new(1.5, p.accent), egui::StrokeKind::Outside);

                ui.scope_builder(egui::UiBuilder::new().max_rect(modal_rect.shrink(18.0)), |ui| {
                    ui.vertical(|ui| {
                        theme::section_label(ui, p, "01 / SUPER-RISOLUZIONE AI");
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("✨ Ricostruzione SOTA Immagine Perfetta ({}×)", self.scale)).size(16.0).strong().color(p.text));
                        ui.add_space(8.0);

                        let is_cpu = self.force_cpu || !GPU.is_gpu_acceleration_available();
                        ui.horizontal_wrapped(|ui| {
                            if is_cpu {
                                theme::chip(ui, "HARDWARE: CPU", Color32::from_rgb(240, 160, 40));
                                theme::chip(ui, "PIPELINE SOTA (6 STADI)", p.muted);
                            } else {
                                theme::chip(ui, &format!("GPU: {}", GPU.name), p.accent);
                                theme::chip(ui, "PIPELINE SOTA (6 STADI)", p.on_accent);
                            }
                            theme::chip(ui, &format!("SCALA: {}×", self.scale), p.muted);
                        });
                        ui.add_space(14.0);

                        // Barra animata ArtCraft
                        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 4.0, p.panel_alt);

                        let time = ui.input(|i| i.time);
                        let progress = ((time * 1.4).sin().abs() as f32).clamp(0.05, 0.95);
                        let active_w = (bar_rect.width() * 0.4).max(60.0);
                        let offset_x = (bar_rect.width() - active_w) * progress;
                        let active_rect = egui::Rect::from_min_size(
                            bar_rect.min + Vec2::new(offset_x, 0.0),
                            Vec2::new(active_w, bar_rect.height()),
                        );
                        let bar_color = if is_cpu { Color32::from_rgb(240, 160, 40) } else { p.accent };
                        ui.painter().rect_filled(active_rect, 4.0, bar_color);

                        ui.add_space(10.0);
                        let msg = if is_cpu {
                            "Elaborazione su processore CPU in corso (potrebbe richiedere tempo)…"
                        } else {
                            "Elaborazione tensoriale GPU in corso…"
                        };
                        ui.label(RichText::new(msg).small().color(p.text));
                        ui.label(RichText::new("Ricostruzione micro-dettagli, trame e riduzione rumore").small().color(p.muted));

                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕ Annulla").clicked() {
                                self.upscaling = None;
                            }
                        });
                    });
                });
            }

            // frecce
            for (dx, label, d) in [(16.0, "‹", -1), (screen.width() - 56.0, "›", 1)] {
                let r = egui::Rect::from_min_size(egui::pos2(screen.min.x + dx, screen.center().y - 24.0), Vec2::new(40.0, 48.0));
                if self.items.len() > 1 && ui.put(r, egui::Button::new(RichText::new(label).size(26.0))).clicked() {
                    self.step(d);
                }
            }
        });
        open
    }

    /// Immagine adattata allo schermo, con zoom (rotella) e spostamento (trascinamento); doppio clic = adatta.
    fn image(&mut self, ui: &mut egui::Ui, t: &egui::TextureHandle, stage: egui::Rect) {
        let size = t.size_vec2();
        let scale_to_fit = ((stage.width() - 40.0) / size.x).min((stage.height() - 20.0) / size.y);
        // Se l'immagine è una miniatura o molto piccola (< 320px), la scaliamo fino a riempire lo stage
        // così coincide esattamente con i limiti dell'immagine ad alta risoluzione senza salti o sfarfallii.
        let fit = if size.x < 320.0 || size.y < 320.0 {
            scale_to_fit
        } else {
            scale_to_fit.min(1.0)
        };
        let resp = ui.interact(stage, ui.make_persistent_id("imancraft-viewer-img"), egui::Sense::click_and_drag());
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.0015)).clamp(0.5, 8.0);
            }
        }
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.double_clicked() {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        let rect = egui::Rect::from_center_size(stage.center() + self.pan, size * fit * self.zoom);
        ui.painter_at(stage).image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
}
