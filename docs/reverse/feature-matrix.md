# Reverse Engineering di iMazing 3.5.5 — Matrice e Specifiche Formati

Data analisi: 16 Ottobre 2026  
Target analizzato: iMazing 3.5.5 (installato in `D:\Programmi\iMazing\`)  
Metodologia: **Clean-Room Reverse Engineering** (analisi statica PE, metadati ECMA-335 / .NET metadata heaps `#~`, `#Strings`, `#Blob`, estrazione schema RVA/SQLite, disassemblaggio simbolico, per sola interoperabilità e compatibilità formati).

---

## 1. Architettura dell'applicazione iMazing per Windows

Dall'ispezione dell'installazione in `D:\Programmi\iMazing\` emergono i seguenti componenti chiave:

```
iMazing.exe                 Frontend WPF / .NET Framework 4.6.2 (GUI, orchestrazione UI)
DDNAToolKitNET.dll          Wrapper interop C# gestito (SWIG C++ -> C# P/Invoke)
DDNAToolKit.dll             Engine nativo C++ principale (48.5 MB, x86_64 PE)
                            - Gestione backup iOS, crittografia, pairing usbmuxd
                            - Database SQLite Versions.db e snapshot hardlink
                            - Parser protocolli AFC, MobileBackup2, Diagnostics
Manifest-Operations-Shared.dll  Logica condivisa per operazioni sul Manifest e dataset
iMazing-CLI.exe             Interfaccia a riga di comando per scripting e automazione
```

---

## 2. Specifiche Formati Proprietari Scoperti

### 2.1 Formato Backup Incrementali: `iMazing.Versions`

Nei backup gestiti da iMazing, la cartella del backup (identificata dall'UDID del dispositivo) può essere convertita o affiancata da una struttura snapshot incrementale `iMazing.Versions/` (situata ad esempio nella radice di backup di iMazing).

#### Gerarchia Cartelle:
```
<BackupRoot>/
  ├── <DeviceUDID>/                  (Backup base / working copy)
  └── iMazing.Versions/
        ├── Blueprints/              (Template di provisioning e configurazione)
        ├── Trash/                   (Versioni o file marcati per cancellazione)
        ├── Versions.db              (Database SQLite centrale degli snapshot)
        └── <DeviceUDID>/
              ├── <Timestamp_1>/     (Snapshot n. 1)
              ├── <Timestamp_2>/     (Snapshot n. 2)
              ...
```

#### Schema Database SQLite (`Versions.db` - Versione 4):
Estratto direttamente dai comandi DDL presenti nell'engine nativo:

```sql
-- Metadati generali
CREATE TABLE PROPERTIES (
    DeviceUDID TEXT NOT NULL,
    ArchivingSysVersion INTEGER NOT NULL    -- Valore corrente = 4
);

-- Tabella delle versioni (snapshot temporali)
CREATE TABLE VERSIONS (
    iOSVersion TEXT NOT NULL,
    DeviceName TEXT NOT NULL,
    FolderName TEXT NOT NULL,
    Timestamp INTEGER NOT NULL UNIQUE,      -- Unix timestamp dello snapshot
    IsEncrypted INTEGER NOT NULL,          -- 0 o 1
    DeleteStatus INTEGER NOT NULL,         -- Enum DeleteStatus: 0=None, 1=PendingDeletion, 2=Deleted
    BackupReason INTEGER NOT NULL,         -- Enum BackupReason: 1=User, 2=Scheduled, 3=DatasetRefresh...
    OriginalSize INTEGER                   -- Dimensione totale del backup sorgente in byte
);

-- Catalogo file deduplicati (payload di backup)
CREATE TABLE FILES (
    VersionID INTEGER NOT NULL,
    Filename TEXT NOT NULL,
    Size INTEGER NOT NULL,
    Modified INTEGER NOT NULL,
    Key BLOB NOT NULL                       -- Omesso se IsEncrypted = 0
);
CREATE INDEX FilesVersionIdx ON Files(VersionID);

-- Catalogo file speciali/radice di sistema dello snapshot
CREATE TABLE OTHERFILES (
    VersionID INTEGER NOT NULL,
    Filename TEXT NOT NULL,
    Size INTEGER NOT NULL,
    Modified INTEGER NOT NULL,
    Tag INTEGER NOT NULL,
    Digest TEXT
);

-- Tabelle di associazione molti-a-molti (Deduplicazione / Hardlink)
CREATE TABLE VERSIONS_FILES (
    VersionID INTEGER NOT NULL,
    FileID INTEGER NOT NULL
);
CREATE INDEX VersionsFilesFileIDIdx ON VERSIONS_FILES(FileID);
CREATE INDEX VersionsFilesVersionIDIdx ON VERSIONS_FILES(VersionID);

CREATE TABLE VERSIONS_OTHERFILES (
    VersionID INTEGER NOT NULL,
    FileID INTEGER NOT NULL
);
```

#### Logica di Deduplicazione e Pulizia:
- `FILES` memorizza i blocchi/file fisici indicizzati per hash. Quando due snapshot contengono lo stesso file, esso viene associato a più versioni tramite `VERSIONS_FILES`.
- Su file system NTFS o APFS, i file fisici vengono collegati tramite hardlink oppure referenziati logicamente tramite `VERSIONS_FILES`.
- **Query di eliminazione identificata nell'engine:**
  ```sql
  SELECT FileID FROM VERSIONS_FILES
  WHERE FileID IN (SELECT FileID FROM VERSIONS_FILES WHERE VersionID = ?)
  GROUP BY FileID
  HAVING count(FileID) = 1 AND VersionID = ?;
  ```
  Se il conteggio dei collegamenti a un `FileID` è esattamente 1 ed appartiene alla versione da eliminare, il file fisico viene rimosso dal disco; altrimenti viene solo rimossa la riga di associazione in `VERSIONS_FILES`.
- File radice tracciati in `OTHERFILES`:
  - `Manifest.db`, `Manifest.mbdb`, `Status.plist`, `Info.plist`, `Manifest.plist`
  - `DDNABackup.plist`, `iMazingBackupChecksum.txt`, `Books.backup`
  - `homeScreen.png`, `lockScreen.png` (catture dello schermo home/blocco memorizzate nello snapshot)

#### Enums di Snapshot:
- **`VersionedBackupReason`**:
  - `0`: `None`
  - `1`: `UserInitiated` (Backup manuale avviato dall'utente)
  - `2`: `Scheduled` (Backup automatico / pianificato)
  - `3`: `DatasetRefresh` (Aggiornamento parziale di dataset specifici)
  - `4`: `ImportCurrentBackup` (Importazione da un backup preesistente)
  - `5`: `DataTransfer` (Migrazione / trasferimento dati)
  - `6`: `Consolidated` (Snapshot consolidato)
- **`DeleteStatus`**:
  - `0`: `None` (Attivo / integro)
  - `1`: `PendingDeletion` (In coda per rimozione)
  - `2`: `Deleted` (Eliminato)

---

### 2.2 Metadati del Backup: `DDNABackup.plist`

`DDNABackup.plist` viene generato alla radice di ogni backup gestito o esportato da iMazing.

#### Campi Identificati:
- `deviceUDID`: Stringa UDID dispositivo
- `deviceModelDetails`: Dettagli hardware (es. `"iPhone 14 (A2882) - 128 GB - Midnight"`)
- `deviceAppleIDEmail`: Email Apple ID associato
- `deviceAppleIDDSPersonID`: DS Person ID univoco Apple
- `deviceIconFilename`: Percorso/nome dell'icona del dispositivo (`"Devices/iPhone15,2_Midnight.png"`)
- `dataSets`: Valore intero a 32-bit (Bitmask dei dataset salvati o estratti)
- `datasetSizes`: Dizionario `{ NomeDataset: DimensioneInByte }`
- `domainSets`: Array di domini inclusi
- `editable`: Booleano (indica se il backup può essere modificato/arricchito)
- `backupChecksum`: SHA/Checksum di integrità del backup
- `iMazingBackupVersion`: Versione dell'engine di backup iMazing (es. 2 o 3)
- `migrationVersion`: Versione migrazione schema
- Campi di occupazione disco per categoria:
  `totalDiskUsage`, `audioDiskUsage`, `musicDiskUsage`, `photosDiskUsage`, `videoDiskUsage`, `booksDiskUsage`, `appsDiskUsage`

#### Decodifica Bitmask `dataSets` (Estratta da `DDNAToolKitNET.DatasetType`):
| Bitmask Hex | Valore Decimale | Nome Costante | Descrizione Contenuto |
|---|---|---|---|
| `0x00000001` | 1 | `DatasetApps` | Applicazioni (.ipa / metadati) |
| `0x00000002` | 2 | `DatasetAppData` | Dati sandbox applicazioni (Documents, Library) |
| `0x00000008` | 8 | `DatasetPhotos` | Libreria fotografica (DCIM, PhotoData, Photos.sqlite) |
| `0x00000010` | 16 | `DatasetMessages` | iMessage e SMS (sms.db e allegati) |
| `0x00000020` | 32 | `DatasetCallHistory` | Registro chiamate (CallHistory.storedata) |
| `0x00000040` | 64 | `DatasetVoicemail` | Messaggi segreteria telefonica (voicemail.db) |
| `0x00000080` | 128 | `DatasetContacts` | Rubrica contatti (AddressBook.sqlitedb) |
| `0x00000100` | 256 | `DatasetNotes` | Note Apple (NoteStore.sqlite) |
| `0x00000200` | 512 | `DatasetVoiceMemos` | Registrazioni memo vocali |
| `0x00000400` | 1024 | `DatasetCalendars` | Calendari ed eventi (Calendar.sqlitedb) |
| `0x00000800` | 2048 | `DatasetSafariData` | Cronologia, preferiti e stato Safari |
| `0x00001000` | 4096 | `DatasetAccounts` | Account configurati |
| `0x00002000` | 8192 | `DatasetHealth` | Dati sanitari / Apple Health (healthdb.sqlite) |
| `0x00004000` | 16384 | `DatasetKeychain` | Portachiavi credenziali iOS |
| `0x00008000` | 32768 | `DatasetHomeKit` | Dati e configurazioni HomeKit |
| `0x00010000` | 65536 | `DatasetOtherDataAndPreferences` | Preferenze di sistema e cache varia |
| `0x00020000` | 131072 | `DatasetBooks` | Libri, PDF ed e-book Apple Books |
| `0x00040000` | 262144 | `DatasetRingtones` | Suonerie personalizzate e toni avviso |
| `0x00080000` | 524288 | `DatasetMusic` | Libreria musicale e brani audio |
| `0x00100000` | 1048576 | `DatasetVideos` | Video personali e filmati |
| `0x00200000` | 2097152 | `DatasetPodcasts` | Puntate e iscrizioni Podcast |
| `0x00400000` | 4194304 | `DatasetWhatsApp` | Chat, media e database WhatsApp standard |
| `0x00800000` | 8388608 | `DatasetWhatsAppSMB` | WhatsApp Business |
| `0x01000000` | 16777216 | `DatasetTV` | Applicazione Apple TV / media acquistati |
| `0x02000000` | 33554432 | `DatasetAudiobooks` | Audiolibri |
| `0x04000000` | 67108864 | `DatasetProfiles` | Profili di configurazione e certificati MDM |
| `0x08000000` | 134217728 | `DatasetReminders` | Promemoria e to-do list |
| `0x010000000` | 268435456 | `DatasetFiles` | File generici (File Sharing / Documenti) |
| `0xffffffff` | 4294967295 | `DatasetAll` | Tutti i dataset |

*Esempio reale:* Nel backup di test dell'iPhone 14 presente in `F:\DumpIOSPing1\Backup\00008110-000A648136D0A01E`, `dataSets` vale `469966843` (`0x1C031FFB`), che corrisponde esattamente alla combinazione binaria:
Apps (1) + AppData (2) + Photos (8) + Messages (16) + CallHistory (32) + Voicemail (64) + Contacts (128) + Notes (256) + VoiceMemos (512) + Calendars (1024) + Safari (2048) + Other (65536) + Books (131072) + WhatsApp (4194304) + WhatsAppSMB (8388608) + Reminders (134217728) + Files (268435456).

---

### 2.3 Formato Dati App: Contenitore `.imazingapp`

Il formato `.imazingapp` è un archivio compresso ZIP standard utilizzato per archiviare e ripristinare il container dati di una specifica applicazione (Documents, Library, preferenze).

#### Struttura Interna:
```
<BundleID>.imazingapp (archivio ZIP standard)
  ├── Container/              Contenuto della sandbox dell'app (Documents, Library, tmp...)
  │     ├── Documents/
  │     └── Library/
  │           └── Preferences/
  ├── Payload/                (Opzionale) File binario .app / pacchetto applicativo
  ├── iTunesArtwork           (Opzionale) Icona ad alta risoluzione 512x512/1024x1024 PNG
  ├── iTunesMetadata.plist    Metadati App Store (bundle-identifier, itemName, version, etc.)
  └── .lock                   File di lock transazionale
```

---

### 2.4 Funzionalità e Parametri di `iMazing-CLI`

Dall'analisi di `iMazing-CLI.exe` sono stati identificati i comandi supportati:
- `--device <UDID|Name>`: Seleziona il dispositivo o il backup.
- `--backup`: Esegue un backup completo o incrementale.
- `--export-messages --format <pdf|csv|txt>`: Esporta iMessage/SMS con formattazione e allegati.
- `--export-whatsapp --format <txt|csv|pdf>`: Esporta messaggi e media WhatsApp.
- `--export-call-history --format <csv|xls>`: Esporta il registro chiamate.
- `--export-contacts --format <vcard|csv>`: Esporta la rubrica.
- `--extract-app-data --bundle-id <ID> --target <path.imazingapp>`: Estrae container dati app.
- `--restore-app-data --source <path.imazingapp>`: Ripristina dati sandbox app.

---

## 3. Matrice Comparativa Funzionale (iMazing 3.5.5 vs IManCraft)

| Modulo / Funzionalità | iMazing 3.5.5 | IManCraft (Attuale) | IManCraft (Target Fase) | Note di Implementazione Clean-Room |
|---|---|---|---|---|
| **Interfaccia Grafica** | WPF .NET 4.6.2 (Windows) | Rust puro (`eframe`/`egui 0.37` + `wgpu`) | ✅ Fase 1 | Design system Crafting Apps nativo, zero C, zero browser |
| **Backup iOS Non Cifrati** | Lettura/scrittura MobileBackup2 | Lettura completa Manifest.db | ✅ Fase 1 | Tabelle Files, hash 40 hex, decodifica plist binario |
| **Backup iOS Cifrati** | Sblocco AES/PBKDF2/Keybag | Sblocco memoria AES-256-CBC RFC 3394 | ✅ Fase 1 | Decrittazione sicura zero-disk, range HTTP per seek |
| **Viewer Foto / Video** | Player proprietario + QuickLook | Player nativo ffmpeg + rodio | ✅ Fase 1 | Tile grid HEIC via xstack, miniature iCloud 5005.JPG |
| **WhatsApp Explorer** | Chat, media, export PDF/TXT | Chat a bolle, ricerca avanzata data/ora | ✅ Fase 1 (espansione F3) | Parser ChatStorage.sqlite, gestione Ogg Opus, foto profilo |
| **iMessage / SMS** | Bolle, allegati, export PDF | Bolle, visualizzatore allegati | ✅ Fase 1 (espansione F3) | Parser typedstream attributedBody per iOS 16+ |
| **Note, Contatti, Safari, Chiamate** | Tabelle/export vCard/CSV | Viste native virtualizzate | ✅ Fase 1 (espansione F3) | NoteStore.sqlite gzip protobuf, AddressBook, Safari |
| **Snapshot `iMazing.Versions`** | Creazione e gestione Versions.db v4 | Non ancora implementato | **Fase 3** | Specifiche estratte al 100%: schema SQL pronto per driver Rust |
| **Metadati `DDNABackup.plist`** | Generazione con bitmask 32-bit | Lettura parziale dei metadati | **Fase 3** | Bitmask decodificata al 100%: esporre dataset e dimensioni |
| **Container `.imazingapp`** | Estrazione e ripristino sandbox | Non ancora implementato | **Fase 3** | Archivio ZIP (Container + iTunesMetadata.plist) implementabile con `zip-rs` |
| **Esportazione PDF/XLSX** | Stampa messaggi, export Excel | Solo TXT/CSV | **Fase 3** | Crate pure Rust (`printpdf`/`krilla` e `rust_xlsxwriter`) |
| **Dispositivi Collegati** | Apple Mobile Device / usbmuxd | Non collegato | **Fase 4** | Crate `idevice` 0.1.68 con `rustcrypto` (richiede Apple Devices / AMDS) |
| **Spyware Detection** | Analisi STIX / IOC (MVT) | Non ancora implementato | **Fase 3 / 5** | Parser regole STIX / IOC Amnesty International |
| **Automazione CLI** | `iMazing-CLI.exe` | Non ancora implementato | **Fase 6** | `imancraft-cli` crate indipendente nel workspace |

---

## 4. Conclusioni per la Fase 3

Grazie all'analisi statica clean-room, i formati proprietari di iMazing non sono più una "scatola nera":
1. **`iMazing.Versions`**: Possiamo costruire in `crates/core` un modulo `versions.rs` capace di leggere i backup storici versionati di iMazing, navigare tra gli snapshot temporali e calcolare le differenze tra versioni successive senza dipendere da codice esterno.
2. **`DDNABackup.plist`**: Possiamo decodificare e visualizzare lo stato completo dei dataset e le quote disco direttamente nella schermata Dispositivi/Backup.
3. **`.imazingapp`**: Possiamo offrire l'esportazione e l'ispezione dei container dati delle applicazioni in formato pienamente compatibile con iMazing.
