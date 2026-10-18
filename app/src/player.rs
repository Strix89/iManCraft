//! Lettore video/audio nativo: ffmpeg decodifica il video in frame RGBA (→ texture egui) e l'audio in PCM (→ rodio).
//! Orologio comune a tempo reale; pausa = si ferma l'orologio e l'audio; seek = si riavviano i due ffmpeg con -ss.
//! ponytail: sincronia A/V approssimata (partenza simultanea), sufficiente per clip da telefono.

use egui::{ColorImage, TextureHandle, TextureOptions};
use imancraft_core::media::{self, Probe, AUDIO_CHANNELS, AUDIO_RATE};
use std::io::Read;
use std::num::NonZero;
use std::process::Child;
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::Instant;

/// Lato massimo dei frame decodificati (oltre non serve a schermo e costa CPU).
const MAX_SIDE: u32 = 1280;

pub struct Player {
    url: String,
    pub probe: Probe,
    w: u32,
    h: u32,
    base: f64,
    started: Option<Instant>,
    frames: Option<Receiver<(f64, Vec<u8>)>>,
    pcm: Option<Receiver<Vec<f32>>>,
    next: Option<(f64, Vec<u8>)>,
    tex: Option<TextureHandle>,
    children: Vec<Child>,
    sink: Option<rodio::MixerDeviceSink>,
    audio: Option<rodio::Player>,
    pub volume: f32,
    seeking: Option<f64>,
    pub error: Option<String>,
}

impl Player {
    /// Apre e avvia la riproduzione (`url` dal server media interno).
    pub fn open(url: String) -> Player {
        let probe = media::probe(&url).unwrap_or_default();
        let scale = (MAX_SIDE as f64 / probe.width.max(probe.height).max(1) as f64).min(1.0);
        let even = |v: f64| ((v * scale) as u32 / 2 * 2).max(2);
        let mut p = Player {
            w: even(probe.width as f64),
            h: even(probe.height as f64),
            url,
            probe,
            base: 0.0,
            started: None,
            frames: None,
            pcm: None,
            next: None,
            tex: None,
            children: vec![],
            sink: None,
            audio: None,
            volume: 1.0,
            seeking: None,
            error: None,
        };
        if !p.probe.has_video && !p.probe.has_audio {
            p.error = Some("file non riproducibile (ffmpeg non lo riconosce)".into());
        } else {
            p.start(0.0);
        }
        p
    }

    pub fn position(&self) -> f64 {
        let pos = self.base + self.started.map_or(0.0, |s| s.elapsed().as_secs_f64());
        if self.probe.duration > 0.0 { pos.min(self.probe.duration) } else { pos }
    }

    pub fn playing(&self) -> bool {
        self.started.is_some()
    }

    fn stop_children(&mut self) {
        for mut c in self.children.drain(..) {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.frames = None;
        self.pcm = None;
        self.next = None;
        self.audio = None;
    }

    fn start(&mut self, pos: f64) {
        self.stop_children();
        self.base = pos;
        if self.probe.has_video {
            match media::spawn_video(&self.url, pos, self.w, self.h) {
                Ok(mut child) => {
                    let mut out = child.stdout.take().unwrap();
                    let (tx, rx) = sync_channel(4);
                    let (size, fps) = ((self.w * self.h * 4) as usize, self.probe.fps);
                    std::thread::spawn(move || {
                        for i in 0u64.. {
                            let mut buf = vec![0u8; size];
                            if out.read_exact(&mut buf).is_err() || tx.send((pos + i as f64 / fps, buf)).is_err() {
                                break;
                            }
                        }
                    });
                    self.frames = Some(rx);
                    self.children.push(child);
                }
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        if self.probe.has_audio {
            if self.sink.is_none() {
                self.sink = rodio::DeviceSinkBuilder::open_default_sink().ok().map(|mut s| {
                    s.log_on_drop(false);
                    s
                });
            }
            if let (Some(sink), Ok(mut child)) = (&self.sink, media::spawn_audio(&self.url, pos)) {
                let player = rodio::Player::connect_new(sink.mixer());
                player.set_volume(self.volume);
                let mut out = child.stdout.take().unwrap();
                // canale limitato: ffmpeg rallenta invece di decodificare tutto l'audio in RAM
                let (tx, rx) = sync_channel::<Vec<f32>>(8);
                // lettura del PCM su un thread: blocchi da 0,1 s
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; (AUDIO_RATE as usize / 10) * AUDIO_CHANNELS as usize * 4];
                    loop {
                        let mut n = 0;
                        while n < buf.len() {
                            match out.read(&mut buf[n..]) {
                                Ok(0) | Err(_) => break,
                                Ok(k) => n += k,
                            }
                        }
                        if n == 0 {
                            break;
                        }
                        let samples: Vec<f32> = buf[..n - n % 4].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                        if tx.send(samples).is_err() || n < buf.len() {
                            break;
                        }
                    }
                });
                self.audio = Some(player);
                self.children.push(child);
                self.pcm = Some(rx);
            }
        }
        self.started = Some(Instant::now());
    }

    /// Passa i blocchi PCM arrivati al player audio (chiamato ogni frame).
    fn pump_audio(&mut self) {
        if let (Some(rx), Some(player)) = (&self.pcm, &self.audio) {
            // al massimo ~2 s in coda nel player (blocchi da 0,1 s)
            while player.len() < 20 {
                let Ok(samples) = rx.try_recv() else { break };
                player.append(rodio::buffer::SamplesBuffer::new(NonZero::new(AUDIO_CHANNELS).unwrap(), NonZero::new(AUDIO_RATE).unwrap(), samples));
            }
        }
    }

    pub fn toggle(&mut self) {
        if let Some(s) = self.started.take() {
            self.base += s.elapsed().as_secs_f64();
            if let Some(a) = &self.audio {
                a.pause();
            }
        } else if self.probe.duration > 0.0 && self.position() >= self.probe.duration - 0.05 {
            self.start(0.0); // ricomincia dalla fine
        } else {
            self.started = Some(Instant::now());
            if let Some(a) = &self.audio {
                a.play();
            }
        }
    }

    pub fn seek(&mut self, pos: f64) {
        let was_playing = self.playing();
        self.start(pos.max(0.0));
        if !was_playing {
            self.toggle();
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v;
        if let Some(a) = &self.audio {
            a.set_volume(v);
        }
    }

    /// Avanzamento per frame: alimenta l'audio, gestisce la fine, chiede il ridisegno mentre suona.
    fn tick(&mut self, ctx: &egui::Context) {
        self.pump_audio();
        if self.playing() && self.probe.duration > 0.0 && self.position() >= self.probe.duration {
            self.toggle(); // fine
        }
        if self.playing() {
            ctx.request_repaint();
        }
    }

    /// Aggiorna il frame corrente; restituisce la texture da disegnare.
    fn update(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        self.tick(ctx);
        let pos = self.position();
        if let Some(rx) = &self.frames {
            loop {
                if self.next.is_none() {
                    self.next = rx.try_recv().ok();
                }
                match &self.next {
                    Some((pts, _)) if *pts <= pos + 0.001 || self.tex.is_none() => {
                        let (_, data) = self.next.take().unwrap();
                        let img = ColorImage::from_rgba_unmultiplied([self.w as usize, self.h as usize], &data);
                        match &mut self.tex {
                            Some(t) => t.set(img, TextureOptions::LINEAR),
                            None => self.tex = Some(ctx.load_texture("player", img, TextureOptions::LINEAR)),
                        }
                    }
                    _ => break,
                }
            }
        }
        self.tex.clone()
    }

    /// Disegna video (se c'è) e controlli. `area` = spazio per il video.
    pub fn ui(&mut self, ui: &mut egui::Ui, area: egui::Vec2) {
        let tex = self.update(ui.ctx());
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {e}"));
            return;
        }
        if self.probe.has_video {
            let size = egui::Vec2::new(self.w as f32, self.h as f32);
            let fit = (area.x / size.x).min(area.y / size.y).min(4.0);
            match tex {
                Some(t) => {
                    ui.add(egui::Image::new(&t).fit_to_exact_size(size * fit));
                }
                None => {
                    ui.add_sized(size * fit, egui::Spinner::new());
                }
            }
        }
        self.controls(ui);
    }

    /// Barra dei controlli: play/pausa, posizione, tempo, volume.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        self.tick(ui.ctx());
        ui.horizontal(|ui| {
            if ui.button(if self.playing() { "⏸" } else { "▶" }).on_hover_text("Play/Pausa (Spazio)").clicked() {
                self.toggle();
            }
            let dur = self.probe.duration.max(0.1);
            let mut pos = self.seeking.unwrap_or_else(|| self.position());
            let w = (ui.available_width() - 190.0).max(80.0);
            ui.spacing_mut().slider_width = w;
            let r = ui.add(egui::Slider::new(&mut pos, 0.0..=dur).show_value(false));
            if r.dragged() {
                self.seeking = Some(pos);
            }
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                self.seeking = None;
                self.seek(pos);
            }
            ui.monospace(format!("{} / {}", fmt_time(pos), fmt_time(dur)));
            if self.probe.width > 0 && self.probe.height > 0 {
                ui.label(egui::RichText::new(format!("{}×{}", self.probe.width, self.probe.height)).small().color(egui::Color32::from_gray(170)));
            }
            let mut v = self.volume;
            ui.spacing_mut().slider_width = 60.0;
            if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false)).on_hover_text("Volume").changed() {
                self.set_volume(v);
            }
        });
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop_children();
    }
}

pub fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
