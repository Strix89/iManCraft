//! Supporto al formato contenitore `.imazingapp` (archivio ZIP standard)
//! utilizzato da iMazing per salvare e ripristinare i dati di sandbox delle singole app iOS.

use crate::{read_plist, txt, Ctx, Res};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Metadati estratti dall'ispezione di un archivio `.imazingapp`
#[derive(Clone, Debug, Default)]
pub struct ImazingAppMeta {
    pub bundle_id: String,
    pub item_name: String,
    pub version: String,
    pub artist_name: String,
    pub file_count: usize,
    pub total_uncompressed_bytes: u64,
    pub has_artwork: bool,
    pub has_payload: bool,
}

/// Esporta il container di un'app presente nel backup in un archivio `.imazingapp` standard.
pub fn export_imazing_app(cx: &Ctx, bundle_id: &str, dest_path: &Path) -> Res<usize> {
    let app_domain = format!("AppDomain-{bundle_id}");

    // 1. Cerca tutti i file appartenenti alla sandbox dell'app
    let mut st = cx.m.prepare(
        "SELECT relativePath, fileID, flags FROM Files
         WHERE domain = ?1 AND flags = 1 AND relativePath <> ''",
    )?;
    let file_rows = st
        .query_map([&app_domain], |r| Ok((txt(r, 0), txt(r, 1))))?
        .collect::<Result<Vec<_>, _>>()?;

    let out_file = File::create(dest_path)?;
    let mut zip = ZipWriter::new(out_file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // 2. Scrive il file di lock transazionale
    zip.start_file(".lock", options)?;

    // 3. Estrae metadati dell'app da Info.plist se disponibili
    if let Some(info_val) = read_plist(&cx.dir.join("Info.plist")) {
        if let Some(apps) = info_val.as_dictionary().and_then(|d| d.get("Applications")?.as_dictionary()) {
            if let Some(app_dict) = apps.get(bundle_id).and_then(|v| v.as_dictionary()) {
                // iTunesMetadata.plist
                if let Some(md_bytes) = app_dict.get("iTunesMetadata").and_then(|v| v.as_data()) {
                    zip.start_file("iTunesMetadata.plist", options)?;
                    zip.write_all(md_bytes)?;
                }

                // iTunesArtwork (PNG)
                if let Some(icon_bytes) = app_dict.get("PlaceholderIcon").and_then(|v| v.as_data()) {
                    zip.start_file("iTunesArtwork", options)?;
                    zip.write_all(icon_bytes)?;
                }
            }
        }
    }

    // 4. Scrive tutti i file del container nella cartella "Container/<relativePath>"
    let mut written = 0;
    for (rel, file_id) in &file_rows {
        // Normalizza il percorso nel container
        let zip_entry_name = format!("Container/{}", rel.trim_start_matches('/'));
        let content = cx.content(file_id)?;
        let bytes = content.read_all()?;

        zip.start_file(zip_entry_name, options)?;
        zip.write_all(&bytes)?;
        written += 1;
    }

    zip.finish()?;
    Ok(written)
}

/// Ispeziona un file `.imazingapp` esistente
pub fn inspect_imazing_app(path: &Path) -> Res<ImazingAppMeta> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut meta = ImazingAppMeta::default();
    let mut metadata_bytes: Option<Vec<u8>> = None;

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if !entry.is_dir() {
            meta.file_count += 1;
            meta.total_uncompressed_bytes += entry.size();
        }

        if name == "iTunesArtwork" {
            meta.has_artwork = true;
        } else if name.starts_with("Payload/") {
            meta.has_payload = true;
        } else if name == "iTunesMetadata.plist" {
            let mut buf = Vec::with_capacity(entry.size() as usize);
            let mut reader = entry;
            reader.read_to_end(&mut buf)?;
            metadata_bytes = Some(buf);
        }
    }

    if let Some(bytes) = metadata_bytes {
        if let Ok(plist_val) = plist::Value::from_reader(Cursor::new(bytes)) {
            if let Some(d) = plist_val.as_dictionary() {
                let get_s = |k| d.get(k).and_then(|v| v.as_string()).unwrap_or_default().to_string();
                meta.bundle_id = get_s("softwareVersionBundleId");
                meta.item_name = get_s("itemName");
                meta.version = get_s("bundleShortVersionString");
                meta.artist_name = get_s("artistName");
            }
        }
    }

    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zip_archive_creation() -> Res<()> {
        let tmp_dir = std::env::temp_dir().join("imancraft_test_imazingapp");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let zip_file = tmp_dir.join("test.imazingapp");

        {
            let out_file = File::create(&zip_file)?;
            let mut zip = ZipWriter::new(out_file);
            let options = SimpleFileOptions::default();

            zip.start_file(".lock", options)?;
            zip.start_file("Container/Documents/notes.txt", options)?;
            zip.write_all(b"Hello from container")?;
            zip.finish()?;
        }

        let info = inspect_imazing_app(&zip_file)?;
        assert_eq!(info.file_count, 2);
        assert!(info.total_uncompressed_bytes >= 20);

        let _ = std::fs::remove_file(&zip_file);
        let _ = std::fs::remove_dir(&tmp_dir);
        Ok(())
    }
}
