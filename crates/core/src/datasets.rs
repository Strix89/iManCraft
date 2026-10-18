//! Decodifica completa di `DDNABackup.plist` e bitmask a 32 bit `dataSets`
//! scoperta tramite reverse engineering di iMazing 3.5.5.

use std::collections::HashMap;

/// Bitmask a 32 bit dei dataset supportati/archiviati in un backup iMazing (`DDNABackup.plist -> dataSets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DatasetFlags(pub u32);

impl DatasetFlags {
    pub const APPS: u32 = 0x0000_0001;
    pub const APP_DATA: u32 = 0x0000_0002;
    pub const PHOTOS: u32 = 0x0000_0008;
    pub const MESSAGES: u32 = 0x0000_0010;
    pub const CALL_HISTORY: u32 = 0x0000_0020;
    pub const VOICEMAIL: u32 = 0x0000_0040;
    pub const CONTACTS: u32 = 0x0000_0080;
    pub const NOTES: u32 = 0x0000_0100;
    pub const VOICE_MEMOS: u32 = 0x0000_0200;
    pub const CALENDARS: u32 = 0x0000_0400;
    pub const SAFARI_DATA: u32 = 0x0000_0800;
    pub const ACCOUNTS: u32 = 0x0000_1000;
    pub const HEALTH: u32 = 0x0000_2000;
    pub const KEYCHAIN: u32 = 0x0000_4000;
    pub const HOME_KIT: u32 = 0x0000_8000;
    pub const OTHER_PREFERENCES: u32 = 0x0001_0000;
    pub const BOOKS: u32 = 0x0002_0000;
    pub const RINGTONES: u32 = 0x0004_0000;
    pub const MUSIC: u32 = 0x0008_0000;
    pub const VIDEOS: u32 = 0x0010_0000;
    pub const PODCASTS: u32 = 0x0020_0000;
    pub const WHATSAPP: u32 = 0x0040_0000;
    pub const WHATSAPP_SMB: u32 = 0x0080_0000;
    pub const APPLE_TV: u32 = 0x0100_0000;
    pub const AUDIOBOOKS: u32 = 0x0200_0000;
    pub const PROFILES: u32 = 0x0400_0000;
    pub const REMINDERS: u32 = 0x0800_0000;
    pub const FILES: u32 = 0x1000_0000;
    pub const ALL: u32 = 0xFFFF_FFFF;

    #[inline]
    pub fn contains(&self, flag: u32) -> bool {
        (self.0 & flag) == flag
    }

    /// Elenco leggibile dei dataset attivi in questo backup.
    pub fn active_datasets(&self) -> Vec<(&'static str, &'static str)> {
        const ALL_FLAGS: &[(u32, &'static str, &'static str)] = &[
            (DatasetFlags::APPS, "App", "Applicazioni"),
            (DatasetFlags::APP_DATA, "Dati app", "Sandbox e documenti delle applicazioni"),
            (DatasetFlags::PHOTOS, "Foto", "Rullino fotografico e video"),
            (DatasetFlags::MESSAGES, "Messaggi", "iMessage e SMS"),
            (DatasetFlags::CALL_HISTORY, "Chiamate", "Registro chiamate"),
            (DatasetFlags::VOICEMAIL, "Segreteria", "Messaggi segreteria telefonica"),
            (DatasetFlags::CONTACTS, "Contatti", "Rubrica telefonica"),
            (DatasetFlags::NOTES, "Note", "Note Apple"),
            (DatasetFlags::VOICE_MEMOS, "Memo vocali", "Registrazioni audio memo vocali"),
            (DatasetFlags::CALENDARS, "Calendari", "Eventi e calendari"),
            (DatasetFlags::SAFARI_DATA, "Safari", "Cronologia e preferiti Safari"),
            (DatasetFlags::ACCOUNTS, "Account", "Account configurati"),
            (DatasetFlags::HEALTH, "Salute", "Dati sanitari Apple Health"),
            (DatasetFlags::KEYCHAIN, "Portachiavi", "Password e certificati"),
            (DatasetFlags::HOME_KIT, "HomeKit", "Configurazioni casa"),
            (DatasetFlags::OTHER_PREFERENCES, "Preferenze", "Altre preferenze di sistema"),
            (DatasetFlags::BOOKS, "Libri", "Apple Books ed e-book"),
            (DatasetFlags::RINGTONES, "Suonerie", "Suonerie personalizzate"),
            (DatasetFlags::MUSIC, "Musica", "Brani e libreria musicale"),
            (DatasetFlags::VIDEOS, "Video", "Filmati personali"),
            (DatasetFlags::PODCASTS, "Podcast", "Puntate e podcast"),
            (DatasetFlags::WHATSAPP, "WhatsApp", "Chat e media WhatsApp standard"),
            (DatasetFlags::WHATSAPP_SMB, "WhatsApp Business", "Account WhatsApp Business"),
            (DatasetFlags::APPLE_TV, "TV", "Contenuti Apple TV"),
            (DatasetFlags::AUDIOBOOKS, "Audiolibri", "Audiolibri"),
            (DatasetFlags::PROFILES, "Profili", "Profili e configurazioni MDM"),
            (DatasetFlags::REMINDERS, "Promemoria", "Promemoria e to-do list"),
            (DatasetFlags::FILES, "File", "Documenti e file generici"),
        ];

        let mut out = Vec::new();
        for &(flag, code, desc) in ALL_FLAGS {
            if self.contains(flag) {
                out.push((code, desc));
            }
        }
        out
    }
}

/// Metadati completi estratti da `DDNABackup.plist`
#[derive(Clone, Debug, Default)]
pub struct DdnaMeta {
    pub udid: String,
    pub model_details: String,
    pub apple_id: String,
    pub icon_filename: String,
    pub flags: DatasetFlags,
    pub dataset_sizes: HashMap<String, u64>,
    pub total_disk_usage: u64,
    pub photos_disk_usage: u64,
    pub music_disk_usage: u64,
    pub video_disk_usage: u64,
    pub audio_disk_usage: u64,
    pub books_disk_usage: u64,
    pub apps_disk_usage: u64,
}

/// Parsing di `DDNABackup.plist`
pub fn parse_ddna_backup(val: &plist::Value) -> Option<DdnaMeta> {
    let dict = val.as_dictionary()?;

    let get_str = |k: &str| dict.get(k).and_then(|v| v.as_string()).unwrap_or_default().to_string();
    let get_u64 = |k: &str| {
        dict.get(k)
            .and_then(|v| v.as_unsigned_integer().or_else(|| v.as_signed_integer().map(|i| i.max(0) as u64)))
            .unwrap_or(0)
    };

    let flags_val = get_u64("dataSets") as u32;

    let mut dataset_sizes = HashMap::new();
    if let Some(sizes_dict) = dict.get("datasetSizes").and_then(|v| v.as_dictionary()) {
        for (k, v) in sizes_dict {
            if let Some(s) = v.as_unsigned_integer().or_else(|| v.as_signed_integer().map(|i| i.max(0) as u64)) {
                dataset_sizes.insert(k.clone(), s);
            }
        }
    }

    Some(DdnaMeta {
        udid: get_str("deviceUDID"),
        model_details: get_str("deviceModelDetails"),
        apple_id: get_str("deviceAppleIDEmail"),
        icon_filename: get_str("deviceIconFilename"),
        flags: DatasetFlags(flags_val),
        dataset_sizes,
        total_disk_usage: get_u64("totalDiskUsage"),
        photos_disk_usage: get_u64("photosDiskUsage"),
        music_disk_usage: get_u64("musicDiskUsage"),
        video_disk_usage: get_u64("videoDiskUsage"),
        audio_disk_usage: get_u64("audioDiskUsage"),
        books_disk_usage: get_u64("booksDiskUsage"),
        apps_disk_usage: get_u64("appsDiskUsage"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dataset_bitmask() {
        // Valore reale dell'iPhone 14 di test: 469966843 (0x1C031FFB)
        let flags = DatasetFlags(469966843);
        assert!(flags.contains(DatasetFlags::APPS));
        assert!(flags.contains(DatasetFlags::APP_DATA));
        assert!(flags.contains(DatasetFlags::PHOTOS));
        assert!(flags.contains(DatasetFlags::MESSAGES));
        assert!(flags.contains(DatasetFlags::CALL_HISTORY));
        assert!(flags.contains(DatasetFlags::VOICEMAIL));
        assert!(flags.contains(DatasetFlags::CONTACTS));
        assert!(flags.contains(DatasetFlags::NOTES));
        assert!(flags.contains(DatasetFlags::VOICE_MEMOS));
        assert!(flags.contains(DatasetFlags::CALENDARS));
        assert!(flags.contains(DatasetFlags::SAFARI_DATA));
        assert!(flags.contains(DatasetFlags::BOOKS));
        assert!(flags.contains(DatasetFlags::OTHER_PREFERENCES));
        assert!(flags.contains(DatasetFlags::PROFILES));
        assert!(flags.contains(DatasetFlags::REMINDERS));
        assert!(flags.contains(DatasetFlags::FILES));

        // Non inclusi in questo specifico backup
        assert!(!flags.contains(DatasetFlags::WHATSAPP));
        assert!(!flags.contains(DatasetFlags::MUSIC));
        assert!(!flags.contains(DatasetFlags::VIDEOS));
        assert!(!flags.contains(DatasetFlags::PODCASTS));

        let active = flags.active_datasets();
        assert!(active.iter().any(|(code, _)| *code == "Foto"));
        assert!(active.iter().any(|(code, _)| *code == "Messaggi"));
        assert!(active.iter().any(|(code, _)| *code == "Libri"));

    }
}
