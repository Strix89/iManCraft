//! Motore di Super-Risoluzione e Miglioramento Immagini con Accelerazione Hardware GPU.
//! Supporta accelerazione GPU (NVIDIA GeForce, AMD Radeon, Intel) via Vulkan (Real-ESRGAN NCNN Vulkan)
//! con fallback algoritmico puro Rust ad altissima qualità (Lanczos3 4× + Contrast Adaptive Sharpening CAS).

use egui::ColorImage;
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub has_vulkan: bool,
    pub is_nvidia: bool,
    pub is_amd: bool,
    pub engine_path: Option<PathBuf>,
}

pub static GPU: LazyLock<GpuInfo> = LazyLock::new(detect_gpu);

/// Rileva la GPU presente sul sistema e la disponibilità del motore neurale Vulkan.
fn detect_gpu() -> GpuInfo {
    let mut name = String::new();

    #[cfg(windows)]
    {
        // Query rapida a Win32_VideoController via PowerShell (senza finestra console)
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", "(Get-CimInstance Win32_VideoController).Name"]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let t = line.trim();
                    if !t.is_empty() {
                        name = t.to_string();
                        break;
                    }
                }
            }
        }
    }

    if name.is_empty() {
        name = "GPU Dedicata / Integrata".to_string();
    }

    let has_vulkan = Path::new(r"C:\Windows\System32\vulkan-1.dll").exists();
    let lower = name.to_lowercase();
    let is_nvidia = lower.contains("nvidia") || lower.contains("geforce") || lower.contains("rtx") || lower.contains("gtx");
    let is_amd = lower.contains("amd") || lower.contains("radeon");

    let engine_path = find_neural_engine();

    GpuInfo {
        name,
        has_vulkan,
        is_nvidia,
        is_amd,
        engine_path,
    }
}

/// Cerca l'eseguibile di Real-ESRGAN Vulkan nella directory dell'app, in tools o nel PATH.
pub fn find_neural_engine() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"tools\upscaler\realesrgan-ncnn-vulkan.exe"),
        PathBuf::from(r".\realesrgan-ncnn-vulkan.exe"),
    ];

    for c in &candidates {
        if c.exists() {
            return Some(std::fs::canonicalize(c).unwrap_or_else(|_| c.clone()));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p1 = dir.join(r"tools\upscaler\realesrgan-ncnn-vulkan.exe");
            if p1.exists() {
                return Some(p1);
            }
            let p2 = dir.join("realesrgan-ncnn-vulkan.exe");
            if p2.exists() {
                return Some(p2);
            }
        }
    }

    // Ricerca in PATH
    #[cfg(windows)]
    {
        let mut cmd = Command::new("where.exe");
        cmd.arg("realesrgan-ncnn-vulkan");
        cmd.creation_flags(0x08000000);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some(first) = stdout.lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

/// Super-risoluzione neurale via Vulkan su GPU (NVIDIA / AMD).
pub fn upscale_neural(
    exe: &Path,
    img: &DynamicImage,
    scale: u32,
    model: &str,
) -> Result<DynamicImage, String> {
    let temp_dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(12345);
    let in_path = temp_dir.join(format!("imancraft_sr_in_{nonce}.png"));
    let out_path = temp_dir.join(format!("imancraft_sr_out_{nonce}.png"));

    img.save_with_format(&in_path, ImageFormat::Png)
        .map_err(|e| format!("Salvataggio temporaneo fallito: {e}"))?;

    let models_dir = exe.parent().map(|p| p.join("models")).unwrap_or_else(|| PathBuf::from("models"));

    let mut cmd = Command::new(exe);
    cmd.args([
        "-i",
        in_path.to_str().unwrap_or(""),
        "-o",
        out_path.to_str().unwrap_or(""),
        "-s",
        &scale.to_string(),
        "-n",
        model,
        "-m",
        models_dir.to_str().unwrap_or("models"),
        "-g",
        "0", // GPU device 0 (NVIDIA / AMD)
    ]);

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW: nessuna finestra cmd

    let out = cmd.output().map_err(|e| format!("Esecuzione motore neurale fallita: {e}"))?;

    let _ = std::fs::remove_file(&in_path);

    if !out.status.success() || !out_path.exists() {
        let err = String::from_utf8_lossy(&out.stderr);
        let _ = std::fs::remove_file(&out_path);
        return Err(format!("Errore elaborazione neurale GPU: {err}"));
    }

    let bytes = std::fs::read(&out_path).map_err(|e| format!("Lettura output super-res fallita: {e}"))?;
    let _ = std::fs::remove_file(&out_path);

    image::load_from_memory(&bytes).map_err(|e| format!("Decodifica output neurale fallita: {e}"))
}

/// Super-risoluzione algoritmica 4× pura (Lanczos3 + Contrast Adaptive Sharpening).
/// Ricostruisce ad altissima frequenza bordi e dettagli senza artefatti di ringing.
pub fn upscale_cas_lanczos(img: &DynamicImage, scale: u32) -> Result<DynamicImage, String> {
    let (orig_w, orig_h) = img.dimensions();
    let target_w = orig_w.saturating_mul(scale);
    let target_h = orig_h.saturating_mul(scale);

    // Pass 1: Ricostruzione sinc finestrata con filtro Lanczos3
    let resampled = img.resize(target_w, target_h, image::imageops::FilterType::Lanczos3);
    let rgba = resampled.to_rgba8();

    // Pass 2: Contrast Adaptive Sharpening (CAS)
    let sharpened = contrast_adaptive_sharpening(&rgba, 0.85);

    Ok(DynamicImage::ImageRgba8(sharpened))
}

/// Filtro Contrast-Adaptive Sharpening (CAS): calcola la dinamica locale di contrasto
/// e applica un fattore laplaciano negativo calibrato per evitare overshoot e aloni.
fn contrast_adaptive_sharpening(img: &RgbaImage, sharpness: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    let sharpness = sharpness.clamp(0.0, 1.0);

    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let c = img.get_pixel(x, y);
            let a = img.get_pixel(x, y - 1);
            let b = img.get_pixel(x - 1, y);
            let d = img.get_pixel(x + 1, y);
            let e = img.get_pixel(x, y + 1);

            let mut r_pix = [0u8; 4];
            r_pix[3] = c[3]; // conserva canale alfa intatto

            for ch in 0..3 {
                let c_v = c[ch] as f32 / 255.0;
                let a_v = a[ch] as f32 / 255.0;
                let b_v = b[ch] as f32 / 255.0;
                let d_v = d[ch] as f32 / 255.0;
                let e_v = e[ch] as f32 / 255.0;

                let min_c = c_v.min(a_v.min(b_v.min(d_v.min(e_v))));
                let max_c = c_v.max(a_v.max(b_v.max(d_v.max(e_v))));

                // Guadagno CAS proporzionale al contrasto locale
                let amp = (min_c.min(1.0 - max_c) / max_c.max(1e-5)).sqrt().clamp(0.0, 1.0);
                let w_cas = -amp * (0.125 + 0.125 * sharpness);

                let sum = a_v + b_v + d_v + e_v;
                let sharp = (c_v + w_cas * sum) / (1.0 + 4.0 * w_cas);
                r_pix[ch] = (sharp.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            out.put_pixel(x, y, Rgba(r_pix));
        }
    }
    out
}

/// Super-risoluzione unificata (tenta GPU neurale Real-ESRGAN, con fallback automatico su Lanczos3+CAS).
pub fn upscale_image(
    img: &DynamicImage,
    gpu: &GpuInfo,
    prefer_neural: bool,
) -> Result<DynamicImage, String> {
    if prefer_neural {
        if let Some(exe) = &gpu.engine_path {
            match upscale_neural(exe, img, 4, "realesrgan-x4plus") {
                Ok(res) => return Ok(res),
                Err(e) => {
                    eprintln!("Real-ESRGAN Vulkan failed ({e}), falling back to Lanczos3+CAS");
                }
            }
        }
    }

    upscale_cas_lanczos(img, 4)
}

/// Converte un DynamicImage in ColorImage per egui.
pub fn to_color_image(img: &DynamicImage) -> ColorImage {
    let rgba = img.to_rgba8();
    ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    )
}

/// Decodifica diretta da byte in memoria in un DynamicImage.
pub fn decode_to_dynamic(bytes: &[u8]) -> Result<DynamicImage, String> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut dec = reader.into_decoder().map_err(|e| e.to_string())?;
    let orient = image::ImageDecoder::orientation(&mut dec).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec).map_err(|e| e.to_string())?;
    img.apply_orientation(orient);
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upscale_cas_lanczos() {
        let mut img = RgbaImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let v = if (x + y) % 2 == 0 { 200 } else { 50 };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let upscaled = upscale_cas_lanczos(&dyn_img, 4).expect("upscale should succeed");
        assert_eq!(upscaled.width(), 64);
        assert_eq!(upscaled.height(), 64);
    }

    #[test]
    fn test_to_color_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(8, 8));
        let ci = to_color_image(&img);
        assert_eq!(ci.size, [8, 8]);
    }

    #[test]
    fn test_gpu_detection() {
        let gpu = &*GPU;
        assert!(!gpu.name.is_empty());
        println!("GPU rilevata: {}, vulkan={}, nvidia={}, amd={}", gpu.name, gpu.has_vulkan, gpu.is_nvidia, gpu.is_amd);
    }
}
