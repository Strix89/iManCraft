//! Cache di texture (miniature e immagini) caricate da un pool di thread.
//! Coda LIFO: le ultime richieste (= ciò che è visibile ora) passano davanti; le richieste per elementi non più
//! disegnati da qualche frame vengono scartate. ponytail: tetto fisso di texture, LRU semplice.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use imancraft_core::{media, Backup};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const CAP: usize = 1500;
const WORKERS: usize = 4;

/// Origine di un'immagine.
#[derive(Clone)]
pub enum Src {
    /// byte già in memoria (es. icona PNG di un'app)
    Bytes(Arc<[u8]>),
    /// file del backup: decodifica diretta (jpg/png/webp/gif) o via ffmpeg (HEIC, primo frame dei video)
    File { backup: Arc<Backup>, id: String, name: String },
}

pub enum Img {
    Loading,
    Ready(TextureHandle),
    Failed(String),
}

enum State {
    Pending,
    Ready(TextureHandle),
    Failed(String),
}

struct Slot {
    state: State,
    used: u64,
}

struct Job {
    key: String,
    src: Src,
    max: u32,
}

struct Inner {
    slots: Mutex<HashMap<String, Slot>>,
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
    frame: AtomicU64,
    ctx: Context,
}

#[derive(Clone)]
pub struct Images(Arc<Inner>);

impl Images {
    pub fn new(ctx: &Context) -> Images {
        let inner = Arc::new(Inner { slots: Default::default(), queue: Default::default(), cv: Condvar::new(), frame: AtomicU64::new(0), ctx: ctx.clone() });
        for _ in 0..WORKERS {
            let inner = inner.clone();
            std::thread::spawn(move || worker(&inner));
        }
        Images(inner)
    }

    /// Da chiamare una volta per frame: avanza l'orologio LRU ed espelle le texture in eccesso.
    pub fn begin_frame(&self) {
        self.0.frame.fetch_add(1, Ordering::Relaxed);
        let mut slots = self.0.slots.lock().unwrap();
        if slots.len() > CAP {
            let mut ages: Vec<(u64, String)> = slots.iter().filter(|(_, s)| !matches!(s.state, State::Pending)).map(|(k, s)| (s.used, k.clone())).collect();
            ages.sort();
            for (_, k) in ages.into_iter().take(slots.len() - CAP + CAP / 5) {
                slots.remove(&k);
            }
        }
    }

    /// Texture per `key` (`max` = lato massimo in pixel, 0 = originale fino a 4096). Avvia il caricamento se serve.
    pub fn get(&self, key: &str, max: u32, src: impl FnOnce() -> Src) -> Img {
        let now = self.0.frame.load(Ordering::Relaxed);
        let mut slots = self.0.slots.lock().unwrap();
        if let Some(s) = slots.get_mut(key) {
            s.used = now;
            return match &s.state {
                State::Pending => Img::Loading,
                State::Ready(t) => Img::Ready(t.clone()),
                State::Failed(e) => Img::Failed(e.clone()),
            };
        }
        slots.insert(key.to_string(), Slot { state: State::Pending, used: now });
        drop(slots);
        self.0.queue.lock().unwrap().push(Job { key: key.to_string(), src: src(), max });
        self.0.cv.notify_one();
        Img::Loading
    }
}

fn worker(inner: &Inner) {
    loop {
        let job = {
            let mut q = inner.queue.lock().unwrap();
            loop {
                if let Some(j) = q.pop() {
                    break j;
                }
                q = inner.cv.wait(q).unwrap();
            }
        };
        // Se l'elemento è già pronto in cache, non ricalcolare
        {
            let slots = inner.slots.lock().unwrap();
            if let Some(s) = slots.get(&job.key) {
                if matches!(s.state, State::Ready(_)) {
                    continue;
                }
            } else {
                continue;
            }
        }
        let state = match decode(&job.src, job.max) {
            Ok(img) => State::Ready(inner.ctx.load_texture(&job.key, img, TextureOptions::LINEAR)),
            Err(e) => State::Failed(e),
        };
        if let Some(s) = inner.slots.lock().unwrap().get_mut(&job.key) {
            s.state = state;
        }
        inner.ctx.request_repaint();
    }
}

/// Tenta di decodificare un buffer di byte immagine con più strategie resilienti.
fn parse_image_bytes(bytes: &[u8]) -> Option<image::DynamicImage> {
    if bytes.is_empty() {
        return None;
    }
    if let Ok(reader) = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format() {
        if let Ok(mut dec) = reader.into_decoder() {
            let orient = image::ImageDecoder::orientation(&mut dec).unwrap_or(image::metadata::Orientation::NoTransforms);
            if let Ok(mut img) = image::DynamicImage::from_decoder(dec) {
                img.apply_orientation(orient);
                return Some(img);
            }
        }
    }
    if let Ok(mut dec) = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)) {
        let orient = image::ImageDecoder::orientation(&mut dec).unwrap_or(image::metadata::Orientation::NoTransforms);
        if let Ok(mut img) = image::DynamicImage::from_decoder(dec) {
            img.apply_orientation(orient);
            return Some(img);
        }
    }
    if let Ok(img) = image::load_from_memory(bytes) {
        return Some(img);
    }
    None
}

/// Carica un DynamicImage a risoluzione piena da una sorgente (usato dal visualizzatore e dall'upscaler).
pub fn load_dynamic_image(src: &Src) -> Result<image::DynamicImage, String> {
    match src {
        Src::Bytes(b) => parse_image_bytes(b).ok_or_else(|| "formato immagine non riconosciuto".into()),
        Src::File { backup, id, name } => {
            let is_video = matches!(name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(), "mp4" | "mov" | "m4v" | "3gp" | "avi");
            // Se non è video: prova prima la decodifica diretta dei byte del backup (immediata)
            if !is_video {
                if let Ok(b) = backup.with(|cx| cx.content(id)?.read_all()) {
                    if let Some(img) = parse_image_bytes(&b) {
                        return Ok(img);
                    }
                }
            }
            // Fallback via ffmpeg solo se il file esiste nel backup (video o formati speciali come HEIC)
            let exists = backup.with(|cx| cx.content(id).map(|_| ())).is_ok();
            if !exists {
                return Err("file non presente nel backup".into());
            }
            let b = media::register(backup);
            let jpg = media::jpeg(&media::url(b, id), name, None).map_err(|e| e.to_string())?;
            parse_image_bytes(&jpg).ok_or_else(|| "decodifica immagine fallita anche con ffmpeg".into())
        }
    }
}

fn decode(src: &Src, max: u32) -> Result<ColorImage, String> {
    let mut img = load_dynamic_image(src)?;
    let limit = if max > 0 { max } else { 4096 };
    if img.width() > limit || img.height() > limit {
        img = img.thumbnail(limit, limit);
    }
    let rgba = img.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// Chiave di cache per un file del backup.
pub fn key(b: &Arc<Backup>, id: &str, max: u32) -> String {
    format!("{:p}:{id}:{max}", Arc::as_ptr(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_memory_thumb() {
        let mut img = image::RgbaImage::new(32, 32);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([255, 0, 0, 255]);
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
        let src = Src::Bytes(Arc::from(bytes.into_boxed_slice()));
        let res = decode(&src, 192);
        assert!(res.is_ok());
    }

    #[test]
    fn test_real_thumb() {
        let path = match std::env::var("IMANCRAFT_TEST_THUMB") {
            Ok(p) => p,
            Err(_) => return,
        };
        if let Ok(bytes) = std::fs::read(path) {
            let res = parse_image_bytes(&bytes);
            assert!(res.is_some(), "Thumb should decode!");
        }
    }
}


