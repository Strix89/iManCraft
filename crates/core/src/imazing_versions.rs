//! Lettura e navigazione degli snapshot incrementali `iMazing.Versions`
//! basati sul database SQLite centrale `Versions.db` (Schema v4).

use crate::{open_db, Res};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Metadati generali di archiviazione estratti dalla tabella PROPERTIES
#[derive(Clone, Debug)]
pub struct VersionsProperties {
    pub device_udid: String,
    pub archiving_sys_version: i64,
}

/// Singolo snapshot registrato nella tabella VERSIONS
#[derive(Clone, Debug)]
pub struct SnapshotVersion {
    pub id: i64,
    pub ios_version: String,
    pub device_name: String,
    pub folder_name: String,
    pub timestamp: i64,
    pub is_encrypted: bool,
    pub delete_status: i64,
    pub backup_reason: i64,
    pub original_size: u64,
}

impl SnapshotVersion {
    /// Ritorna la causale del backup in testo leggibile.
    pub fn reason_label(&self) -> &'static str {
        match self.backup_reason {
            1 => "Manuale (Utente)",
            2 => "Automatico (Pianificato)",
            3 => "Aggiornamento Dataset",
            4 => "Importato da Backup",
            5 => "Trasferimento Dati",
            6 => "Consolidato",
            _ => "Non specificato",
        }
    }

    /// Ritorna lo stato di cancellazione in testo leggibile.
    pub fn status_label(&self) -> &'static str {
        match self.delete_status {
            0 => "Attivo",
            1 => "In eliminazione",
            2 => "Eliminato",
            _ => "Sconosciuto",
        }
    }
}

/// File tracciato in uno snapshot (da FILES o OTHERFILES)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotFile {
    pub file_id: i64,
    pub filename: String,
    pub size: u64,
    pub modified: i64,
    pub is_other: bool,
    pub digest: Option<String>,
}

/// Differenze tra due snapshot
#[derive(Clone, Debug, Default)]
pub struct SnapshotDiff {
    pub added: Vec<SnapshotFile>,
    pub removed: Vec<SnapshotFile>,
    pub modified: Vec<SnapshotFile>,
    pub unchanged_count: usize,
}

/// Cerca `Versions.db` partendo dalla cartella di un backup o dalla radice MobileSync/Backup.
pub fn find_versions_db(dir: &Path) -> Option<PathBuf> {
    // 1. Direttamente dir/iMazing.Versions/Versions.db
    let direct = dir.join("iMazing.Versions").join("Versions.db");
    if direct.is_file() {
        return Some(direct);
    }
    // 2. Se dir è la cartella iMazing.Versions stessa
    let self_db = dir.join("Versions.db");
    if self_db.is_file() && dir.file_name().map_or(false, |n| n == "iMazing.Versions") {
        return Some(self_db);
    }
    // 3. Fratello: dir/../iMazing.Versions/Versions.db
    if let Some(parent) = dir.parent() {
        let sibling = parent.join("iMazing.Versions").join("Versions.db");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    None
}

/// Apre la connessione in sola lettura a `Versions.db`.
pub fn open_versions_db(path: &Path) -> Res<Connection> {
    open_db(path)
}

/// Carica le proprietà generali
pub fn load_properties(conn: &Connection) -> Res<VersionsProperties> {
    let mut st = conn.prepare("SELECT DeviceUDID, ArchivingSysVersion FROM PROPERTIES LIMIT 1")?;
    let props = st.query_row([], |r| {
        Ok(VersionsProperties {
            device_udid: r.get(0)?,
            archiving_sys_version: r.get(1)?,
        })
    })?;
    Ok(props)
}

/// Elenca tutti gli snapshot registrati nel database, ordinati per data decrescente
pub fn list_snapshots(conn: &Connection) -> Res<Vec<SnapshotVersion>> {
    let mut st = conn.prepare(
        "SELECT ROWID, iOSVersion, DeviceName, FolderName, Timestamp, IsEncrypted, DeleteStatus, BackupReason, OriginalSize
         FROM VERSIONS ORDER BY Timestamp DESC",
    )?;
    let rows = st.query_map([], |r| {
        Ok(SnapshotVersion {
            id: r.get(0)?,
            ios_version: r.get(1)?,
            device_name: r.get(2)?,
            folder_name: r.get(3)?,
            timestamp: r.get(4)?,
            is_encrypted: r.get::<_, i64>(5)? != 0,
            delete_status: r.get(6)?,
            backup_reason: r.get(7)?,
            original_size: r.get::<_, Option<i64>>(8)?.unwrap_or(0).max(0) as u64,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Restituisce la lista completa dei file associati a uno snapshot specifico (unendo `FILES` e `OTHERFILES`)
pub fn list_snapshot_files(conn: &Connection, version_id: i64) -> Res<Vec<SnapshotFile>> {
    let mut out = Vec::new();

    // 1. File regolari tramite VERSIONS_FILES -> FILES
    let mut st_files = conn.prepare(
        "SELECT f.ROWID, f.Filename, f.Size, f.Modified
         FROM VERSIONS_FILES vf
         JOIN FILES f ON f.ROWID = vf.FileID
         WHERE vf.VersionID = ?1",
    )?;
    let regular_files = st_files.query_map([version_id], |r| {
        Ok(SnapshotFile {
            file_id: r.get(0)?,
            filename: r.get(1)?,
            size: r.get::<_, i64>(2)?.max(0) as u64,
            modified: r.get(3)?,
            is_other: false,
            digest: None,
        })
    })?;
    for f in regular_files {
        out.push(f?);
    }

    // 2. File radice di sistema tramite VERSIONS_OTHERFILES -> OTHERFILES
    let mut st_other = conn.prepare(
        "SELECT o.ROWID, o.Filename, o.Size, o.Modified, o.Digest
         FROM VERSIONS_OTHERFILES vo
         JOIN OTHERFILES o ON o.ROWID = vo.FileID
         WHERE vo.VersionID = ?1",
    )?;
    let other_files = st_other.query_map([version_id], |r| {
        Ok(SnapshotFile {
            file_id: r.get(0)?,
            filename: r.get(1)?,
            size: r.get::<_, i64>(2)?.max(0) as u64,
            modified: r.get(3)?,
            is_other: true,
            digest: r.get(4)?,
        })
    })?;
    for f in other_files {
        out.push(f?);
    }

    out.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(out)
}

/// Calcola le differenze tra uno snapshot precedente (`old_ver_id`) e uno successivo (`new_ver_id`).
pub fn diff_snapshots(conn: &Connection, old_ver_id: i64, new_ver_id: i64) -> Res<SnapshotDiff> {
    let old_files = list_snapshot_files(conn, old_ver_id)?;
    let new_files = list_snapshot_files(conn, new_ver_id)?;

    let old_map: HashMap<&str, &SnapshotFile> = old_files.iter().map(|f| (f.filename.as_str(), f)).collect();
    let new_map: HashMap<&str, &SnapshotFile> = new_files.iter().map(|f| (f.filename.as_str(), f)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut unchanged_count = 0;

    let all_names: HashSet<&str> = old_map.keys().copied().chain(new_map.keys().copied()).collect();

    for name in all_names {
        match (old_map.get(name), new_map.get(name)) {
            (None, Some(n)) => added.push((*n).clone()),
            (Some(o), None) => removed.push((*o).clone()),
            (Some(o), Some(n)) => {
                if o.size != n.size || o.modified != n.modified || o.digest != n.digest {
                    modified.push((*n).clone());
                } else {
                    unchanged_count += 1;
                }
            }
            (None, None) => {}
        }
    }

    added.sort_by(|a, b| a.filename.cmp(&b.filename));
    removed.sort_by(|a, b| a.filename.cmp(&b.filename));
    modified.sort_by(|a, b| a.filename.cmp(&b.filename));

    Ok(SnapshotDiff {
        added,
        removed,
        modified,
        unchanged_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_versions_db() -> Res<()> {
        unsafe { rusqlite::ffi::sqlite3_initialize() };
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            "CREATE TABLE PROPERTIES ('DeviceUDID' TEXT NOT NULL, 'ArchivingSysVersion' INTEGER NOT NULL);
             INSERT INTO PROPERTIES VALUES ('test-udid-1234', 4);

             CREATE TABLE VERSIONS ('iOSVersion' TEXT NOT NULL, 'DeviceName' TEXT NOT NULL, 'FolderName' TEXT NOT NULL,
                                    'Timestamp' INTEGER NOT NULL UNIQUE, 'IsEncrypted' INTEGER NOT NULL,
                                    'DeleteStatus' INTEGER NOT NULL, 'BackupReason' INTEGER NOT NULL, 'OriginalSize' INTEGER);
             INSERT INTO VERSIONS VALUES ('17.5.1', 'iPad Pro', '2026-01-01', 1704067200, 0, 0, 1, 500000);
             INSERT INTO VERSIONS VALUES ('17.5.2', 'iPad Pro', '2026-02-01', 1706745600, 0, 0, 2, 510000);

             CREATE TABLE FILES ('VersionID' INTEGER NOT NULL, 'Filename' TEXT NOT NULL, 'Size' INTEGER NOT NULL, 'Modified' INTEGER NOT NULL);
             INSERT INTO FILES (ROWID, VersionID, Filename, Size, Modified) VALUES (1, 1, 'dcim/img_0001.jpg', 1000, 1704000000);
             INSERT INTO FILES (ROWID, VersionID, Filename, Size, Modified) VALUES (2, 2, 'dcim/img_0002.jpg', 2000, 1706000000);

             CREATE TABLE OTHERFILES ('VersionID' INTEGER NOT NULL, 'Filename' TEXT NOT NULL, 'Size' INTEGER NOT NULL, 'Modified' INTEGER NOT NULL, 'Tag' INTEGER NOT NULL, 'Digest' TEXT);
             INSERT INTO OTHERFILES (ROWID, VersionID, Filename, Size, Modified, Tag, Digest) VALUES (10, 1, 'Manifest.db', 5000, 1704000000, 1, 'sha1-manifest');

             CREATE TABLE VERSIONS_FILES ('VersionID' INTEGER NOT NULL, 'FileID' INTEGER NOT NULL);
             INSERT INTO VERSIONS_FILES VALUES (1, 1);
             INSERT INTO VERSIONS_FILES VALUES (2, 1); -- condiviso!
             INSERT INTO VERSIONS_FILES VALUES (2, 2); -- aggiunto nella v2

             CREATE TABLE VERSIONS_OTHERFILES ('VersionID' INTEGER NOT NULL, 'FileID' INTEGER NOT NULL);
             INSERT INTO VERSIONS_OTHERFILES VALUES (1, 10);
             INSERT INTO VERSIONS_OTHERFILES VALUES (2, 10);",
        )?;

        let props = load_properties(&conn)?;
        assert_eq!(props.device_udid, "test-udid-1234");
        assert_eq!(props.archiving_sys_version, 4);

        let snaps = list_snapshots(&conn)?;
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].ios_version, "17.5.2");
        assert_eq!(snaps[1].ios_version, "17.5.1");

        let files_v1 = list_snapshot_files(&conn, 1)?;
        assert_eq!(files_v1.len(), 2); // 1 file + 1 other

        let diff = diff_snapshots(&conn, 1, 2)?;
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].filename, "dcim/img_0002.jpg");
        assert_eq!(diff.unchanged_count, 2);

        Ok(())
    }
}
