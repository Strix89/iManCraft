//! Media con ffmpeg/ffprobe (nel PATH): JPEG da HEIC e primo frame dei video, decodifica video (RGBA) e audio (PCM f32).
//!
//! ffmpeg legge i file da un mini server HTTP **interno** su 127.0.0.1 con token casuale (non è l'interfaccia: serve solo
//! a ffmpeg). Così Range/seek e decifratura al volo valgono anche per i backup cifrati e nulla finisce in chiaro su disco.

use crate::{Backup, Res};
use std::hash::{BuildHasher, Hasher};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use tiny_http::{Header, Response, StatusCode};

struct Server {
    port: u16,
    token: String,
    backups: Mutex<Vec<Arc<Backup>>>,
}

static SERVER: OnceLock<Server> = OnceLock::new();

fn server() -> &'static Server {
    SERVER.get_or_init(|| {
        let http = tiny_http::Server::http("127.0.0.1:0").expect("server media interno");
        let port = http.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
        // token non indovinabile da altri processi locali
        let rnd = |n: u64| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(n);
            h.finish()
        };
        let token = format!("{:016x}{:016x}", rnd(1), rnd(std::process::id() as u64));
        std::thread::spawn(move || {
            for req in http.incoming_requests() {
                std::thread::spawn(move || handle(req));
            }
        });
        Server { port, token, backups: Mutex::new(vec![]) }
    })
}

/// Registra un backup presso il server interno; restituisce l'indice da usare in [`url`].
pub fn register(b: &Arc<Backup>) -> usize {
    let mut list = server().backups.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = list.iter().position(|x| Arc::ptr_eq(x, b)) {
        return i;
    }
    list.push(b.clone());
    list.len() - 1
}

/// URL che ffmpeg può leggere per il file `id` del backup `b` (vedi [`register`]).
pub fn url(b: usize, id: &str) -> String {
    let s = server();
    format!("http://127.0.0.1:{}/{}/{b}/{id}", s.port, s.token)
}

fn handle(req: tiny_http::Request) {
    let resp = (|| -> Res<Response<Box<dyn std::io::Read + Send>>> {
        let s = server();
        let mut parts = req.url().trim_start_matches('/').split('/');
        if parts.next() != Some(s.token.as_str()) {
            return Err("token".into());
        }
        let b: usize = parts.next().ok_or("b")?.parse()?;
        let id = parts.next().ok_or("id")?.to_string();
        crate::check_id(&id)?;
        let backup = s.backups.lock().unwrap_or_else(|e| e.into_inner()).get(b).cloned().ok_or("backup")?;
        let range = req.headers().iter().find(|h| h.field.equiv("Range")).map(|h| h.value.to_string());
        backup.with(|cx| {
            let c = cx.content(&id)?;
            let mut headers = vec![Header::from_bytes("Accept-Ranges", "bytes").unwrap()];
            let (status, start, len) = match parse_range(range.as_deref(), c.size) {
                Some((st, l)) => {
                    headers.push(Header::from_bytes("Content-Range", format!("bytes {st}-{}/{}", st + l - 1, c.size)).unwrap());
                    (206, st, l)
                }
                None => (200, 0, c.size),
            };
            Ok(Response::new(StatusCode(status), headers, c.reader(start, len)?, Some(len as usize), None))
        })
    })();
    let _ = match resp {
        Ok(r) => req.respond(r),
        Err(_) => req.respond(Response::empty(404)),
    };
}

/// "bytes=a-b" | "bytes=a-" | "bytes=-n" -> (inizio, lunghezza)
pub fn parse_range(h: Option<&str>, size: u64) -> Option<(u64, u64)> {
    let (a, b) = h?.trim().strip_prefix("bytes=")?.split(',').next()?.split_once('-')?;
    if size == 0 {
        return None;
    }
    let (s, e) = if a.is_empty() {
        (size.saturating_sub(b.parse().ok()?), size - 1)
    } else {
        let s: u64 = a.parse().ok()?;
        (s, if b.is_empty() { size - 1 } else { b.parse::<u64>().ok()?.min(size - 1) })
    };
    (s <= e).then(|| (s, e - s + 1))
}

// ---------------------------------------------------------------- ffmpeg

/// Processo senza finestra di console (l'app è GUI: altrimenti a ogni ffmpeg lampeggia una console).
fn cmd(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.stdin(Stdio::null());
    c
}

fn ffmpeg(args: &[String]) -> Res<Vec<u8>> {
    let out = cmd("ffmpeg").args(["-v", "error", "-nostdin"]).args(args).output().map_err(|e| format!("ffmpeg non trovato nel PATH: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    Ok(out.stdout)
}

fn ffprobe_json(url: &str, extra: &[&str]) -> Res<serde_json::Value> {
    let out = cmd("ffprobe")
        .args(["-v", "error"])
        .args(extra)
        .args(["-show_streams", "-of", "json", url])
        .output()
        .map_err(|e| format!("ffprobe non trovato nel PATH: {e}"))?;
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn rotation(streams: &[serde_json::Value]) -> i64 {
    streams
        .iter()
        .flat_map(|s| s["side_data_list"].as_array().cloned().unwrap_or_default())
        .find_map(|sd| sd["rotation"].as_i64())
        .unwrap_or(0)
}

/// Filtro che ricompone un HEIC: ffmpeg espone la griglia di tile (512x512) come stream group ma la CLI
/// non la unisce, e senza mappa sceglie la gain map HDR in scala di grigi.
fn heic_filter(url: &str) -> Res<String> {
    let j = ffprobe_json(url, &["-f", "mov", "-show_stream_groups"])?;
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let turn = match rotation(&streams) {
        -90 | 270 => ",transpose=1",
        90 | -270 => ",transpose=2",
        180 | -180 => ",hflip,vflip",
        _ => "",
    };
    let grid = j["stream_groups"].as_array().and_then(|gs| gs.iter().find(|g| g["type"] == "Tile Grid")).map(|g| &g["components"][0]);
    let subs = grid.and_then(|g| g["subcomponents"].as_array()).filter(|s| s.len() > 1);
    let (Some(g), Some(subs)) = (grid, subs) else {
        // immagine singola: lo stream a colori più grande (escluse depth/gain map in scala di grigi)
        let s = streams
            .iter()
            .filter(|s| s["codec_type"] == "video" && !s["pix_fmt"].as_str().unwrap_or("").starts_with("gray"))
            .max_by_key(|s| s["width"].as_i64().unwrap_or(0) * s["height"].as_i64().unwrap_or(0))
            .ok_or("nessuna immagine nel file HEIC")?;
        return Ok(format!("[0:{}]null{turn}", s["index"]));
    };
    let inputs: String = subs.iter().map(|s| format!("[0:{}]", s["stream_index"])).collect();
    let layout: Vec<String> = subs.iter().map(|s| format!("{}_{}", s["tile_horizontal_offset"], s["tile_vertical_offset"])).collect();
    Ok(format!(
        "{inputs}xstack=inputs={}:layout={},crop={}:{}:{}:{}{turn}",
        subs.len(),
        layout.join("|"),
        g["width"],
        g["height"],
        g["horizontal_offset"],
        g["vertical_offset"]
    ))
}

/// JPEG da una foto (anche HEIC) o primo frame di un video. `max_w` = larghezza massima (miniature).
pub fn jpeg(url: &str, name: &str, max_w: Option<u32>) -> Res<Vec<u8>> {
    let lower = name.to_ascii_lowercase();
    let scale = max_w.map(|w| format!(",scale='min({w},iw)':-2")).unwrap_or_default();
    let mut args: Vec<String> = if lower.ends_with(".heic") || lower.ends_with(".heif") {
        let f = heic_filter(url)?;
        ["-f", "mov", "-i", url, "-filter_complex", &format!("{f}{scale}")].map(String::from).to_vec()
    } else {
        let mut a = ["-i", url].map(String::from).to_vec();
        if !scale.is_empty() {
            a.extend(["-vf".into(), scale.trim_start_matches(',').to_string()]);
        }
        a
    };
    args.extend(["-frames:v", "1", "-pix_fmt", "yuvj420p", "-c:v", "mjpeg", "-q:v", if max_w.is_some() { "5" } else { "2" }, "-f", "mjpeg", "pipe:1"].map(String::from));
    ffmpeg(&args)
}

/// Informazioni su un video/audio (dimensioni già ruotate come vanno mostrate).
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub has_video: bool,
    pub has_audio: bool,
}

pub fn probe(url: &str) -> Res<Probe> {
    let j = ffprobe_json(url, &["-show_format"])?;
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let v = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
    let rate = |s: &str| -> f64 {
        let (a, b) = s.split_once('/').unwrap_or((s, "1"));
        let (a, b) = (a.parse::<f64>().unwrap_or(0.0), b.parse::<f64>().unwrap_or(1.0));
        if b > 0.0 { a / b } else { 0.0 }
    };
    let (mut w, mut h) = v.map_or((0, 0), |s| (s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32));
    if rotation(&streams).abs() % 180 == 90 {
        std::mem::swap(&mut w, &mut h);
    }
    let duration = j["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0);
    Ok(Probe {
        width: w,
        height: h,
        fps: v.map(|s| rate(s["avg_frame_rate"].as_str().unwrap_or("0"))).filter(|f| *f > 0.0 && *f < 240.0).unwrap_or(30.0),
        duration,
        has_video: v.is_some(),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}

/// ffmpeg che decodifica il video da `start` secondi in frame RGBA `w`×`h` su stdout (rawvideo).
pub fn spawn_video(url: &str, start: f64, w: u32, h: u32) -> Res<Child> {
    let child = cmd("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss", &format!("{start:.3}"), "-i", url, "-map", "0:v:0"])
        .args(["-vf", &format!("scale={w}:{h}"), "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg non trovato nel PATH: {e}"))?;
    Ok(child)
}

/// Frequenza e canali dell'audio prodotto da [`spawn_audio`].
pub const AUDIO_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u16 = 2;

/// ffmpeg che decodifica l'audio da `start` secondi in PCM f32 little-endian stereo 48 kHz su stdout.
pub fn spawn_audio(url: &str, start: f64) -> Res<Child> {
    let child = cmd("ffmpeg")
        .args(["-v", "error", "-nostdin", "-ss", &format!("{start:.3}"), "-i", url, "-map", "0:a:0"])
        .args(["-ac", &AUDIO_CHANNELS.to_string(), "-ar", &AUDIO_RATE.to_string(), "-f", "f32le", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ffmpeg non trovato nel PATH: {e}"))?;
    Ok(child)
}

/// Conversione HEIC → JPEG (piena risoluzione) per l'export.
pub fn heic_to_jpeg(b: usize, id: &str) -> Res<Vec<u8>> {
    jpeg(&url(b, id), "x.heic", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range(Some("bytes=0-"), 10), Some((0, 10)));
        assert_eq!(parse_range(Some("bytes=2-4"), 10), Some((2, 3)));
        assert_eq!(parse_range(Some("bytes=-3"), 10), Some((7, 3)));
        assert_eq!(parse_range(Some("bytes=20-"), 10), None);
    }
}
