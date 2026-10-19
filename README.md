# IManCraft

Client desktop nativo ad alte prestazioni per l'analisi forense, l'esplorazione strutturata e l'estrazione di backup iOS (formato iTunes, Finder, Apple Devices e iMazing).

Progettato interamente in Rust con interfaccia grafica vettoriale nativa basata su egui e backend di rendering wgpu, IManCraft elimina qualsiasi dipendenza da runtime web o framework Electron. Integra un motore proprietario di decifratura crittografica operante esclusivamente in memoria volatile e una pipeline di super-risoluzione e restauro visivo SOTA accelerata via compute shader GPU Vulkan.

[![Language](https://img.shields.io/badge/Language-Rust%202021-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64%20(GNU)-blue.svg)](https://www.microsoft.com/windows)
[![GUI Framework](https://img.shields.io/badge/GUI-egui%200.37%20%2F%20wgpu-purple.svg)](https://github.com/emilk/egui)
[![Acceleration](https://img.shields.io/badge/Compute-Vulkan%201.3%20%2F%20Rayon-green.svg)](https://www.khronos.org/vulkan/)
[![License](https://img.shields.io/badge/License-MIT-brightgreen.svg)](LICENSE)

---

## Indice dei Contenuti

- [Principi di Progettazione Forense](#principi-di-progettazione-forense)
- [Architettura del Sistema](#architettura-del-sistema)
- [Moduli di Analisi Forense](#moduli-di-analisi-forense)
- [Pipeline di Super-Risoluzione SOTA](#pipeline-di-super-risoluzione-sota)
- [Modulo Crittografico e Recupero Note Protette](#modulo-crittografico-e-recupero-note-protette)
- [Interfaccia a Riga di Comando (CLI)](#interfaccia-a-riga-di-comando-cli)
- [Requisiti di Sistema](#requisiti-di-sistema)
- [Compilazione dai Sorgenti](#compilazione-dai-sorgenti)
- [Scorciatoie da Tastiera](#scorciatoie-da-tastiera)
- [Struttura del Repository](#struttura-del-repository)
- [Licenza e Disclaimer Legale](#licenza-e-disclaimer-legale)

---

## Principi di Progettazione Forense

IManCraft è concepito per soddisfare i requisiti di integrità e non-ripudiabilità tipici dell'acquisizione e analisi forense digitale:

- **Garanzia di sola lettura (Read-Only)**: L'applicazione apre tutti i file di sistema, i database SQLite (`Manifest.db`, cataloghi applicativi) e gli archivi di dump impostando il flag SQLite `immutable=1` e descrittori di file in sola lettura. Nessun byte del backup originale viene mai modificato, sovrascritto o rilocato.
- **Zero-Disk Footprint per Backup Cifrati**: La decifratura dei backup protetti da password avviene senza creare file temporanei decifrati su disco. Le chiavi crittografiche (KeyBag) derivate via PBKDF2-HMAC-SHA1/SHA256 e srotolate tramite RFC 3394 risiedono esclusivamente nella memoria volatile dell'eseguibile protetto.
- **Micro-Server Loopback Dedicato per Media Cifrati**: Per consentire a decoder binari esterni (come FFmpeg) di eseguire seek dinamico e riproduzione su flussi multimediali memorizzati all'interno di backup cifrati, il core istanzia un server HTTP interno bound a `127.0.0.1` su porta effimera, protetto da token di autenticazione monouso a 256 bit e decifratura AES-CBC al volo per singolo chunk HTTP `Range`.

---

## Architettura del Sistema

Il progetto è strutturato come workspace Cargo multi-crate ad alta coesione e basso accoppiamento:

```
iManCraft Workspace
├── crates/core (imancraft-core)
│   ├── Analisi e parsing SQLite / bplist / Protobuf
│   ├── Motore crittografico (KeyBag, RFC 3394, AES-GCM, PBKDF2)
│   ├── Parser MP4/MOV per metadati e rotazione matriciale
│   ├── Motore di scansione IOC per indicatori spyware
│   └── Esportatori conformi (XLSX, vCard 3.0, RFC 5545 iCalendar, HTML5)
├── app (imancraft)
│   ├── Shell desktop nativa (eframe, egui 0.37, wgpu)
│   ├── Cache asincrona texture LIFO per miniature ad alto throughput
│   ├── Pipeline neurale e geometrica UltraEnhance (4x / 8x)
│   └── Riproduttore nativo audio/video sincronizzato (rodio + ffmpeg pipe)
└── cli (imancraft-cli)
    └── Strumento a riga di comando per scripting, audit batch e CI/CD
```

---

## Moduli di Analisi Forense

La GUI desktop mette a disposizione 19 sezioni specializzate accessibili tramite navigation rail laterale:

### 1. Discover (Panoramica del Dispositivo)
- Scheda riassuntiva del dispositivo: modello commerciale, identificativo univoco hardware (UDID), versione di iOS, data del backup e stato di crittografia.
- Barra segmentata delle quote di memoria: calcolo dinamico dell'occupazione per categorie logiche (Fotografie, Video, Applicazioni, Tracce Audio, File di Sistema).
- Matrice di Quick Actions per l'accesso istantaneo ai flussi di lavoro più frequenti.

### 2. Dispositivi e Snapshot Versions.db
- Rilevamento automatico dei backup presenti nelle cartelle di sistema e supporto per directory arbitrarie (unità esterne, storage di rete).
- Decodifica della bitmask a 32 bit `dataSets` dello standard DDNA (iMazing Backup Engine).
- Ispezione cronologica degli snapshot incrementali memorizzati in `Versions.db`: visualizzazione della timeline e calcolo differenziale dei file aggiunti, modificati o eliminati tra punti di ripristino distinti.

### 3. Ricerca Globale Unificata
- Motore di indicizzazione e scansione full-text simultanea su tutti i database estratti: Contatti, Note, Calendari, Promemoria, Messaggi SMS/iMessage e conversazioni WhatsApp.

### 4. WhatsApp Forensics
- Ricostruzione cronologica delle conversazioni a bolle responsive, messaggi citati, eventi di sistema e allegati.
- Ispezione multimediale avanzata per flussi video con estrazione istantanea delle dimensioni geometriche e dell'orientamento matriciale tramite parser nativo degli atomi MP4/MOV (`moov`/`trak`/`tkhd`).
- Calcolo automatico della classe di risoluzione (4K, Full HD, HD, SD, qHD) visualizzata tramite chip informativi e badge sulle miniature.
- Riproduzione integrata di memo vocali (.opus / .m4a) con controllo di seek e durata.
- Galleria multimediale separata per visualizzare foto, video, documenti e link aggregati per singola chat.
- Filtro temporale granulare per data e ora esatta con scorciatoie rapide.
- Esportazione forense nei formati: TXT, CSV, Microsoft Excel (XLSX) con stili nativi e HTML5 responsive ottimizzato per la stampa o l'archiviazione PDF.

### 5. Telegram iOS Forensics
- Rilevamento del container dell'applicazione (`ph.telegra.Telegraph`) e dell'App Group condiviso (`group.ph.telegra.Telegraph`).
- Parsing forense delle preferenze memorizzate nei file Property List binari (`CKStartupTime`, `TelegramCacheIndexTimestamp`, configurazioni video e grouping).
- Analisi dei database Postbox (`db_sqlite`) e storage legacy (`tgdata.sqlite`).
- Gestione trasparente ed esplicitazione forense delle politiche di esclusione cloud Apple (`NSURLIsExcludedFromBackupKey`).

### 6. Messaggi (iMessage / SMS)
- Parsing completo del database `sms.db`: conversazioni singole e di gruppo, notifiche di lettura, allegati multimediali e transcodifica miniature.
- Esportazione nei formati Microsoft Excel (XLSX) e documento HTML5 stampabile in scala 1:1.

### 7. Libreria Fotografica e Video
- Esplorazione relazionale di `Photos.sqlite`: album utente, cartelle condivise, elementi preferiti e cestino.
- Gestione degli asset conservati esclusivamente su iCloud: identificazione automatica delle voci prive di master locale, estrazione della miniatura presente nel backup e applicazione di badge distintivo.
- Visualizzatore nativo a schermo intero con zoom centrico vettoriale, pan fluido e navigazione sequenziale.
- Modulo di esportazione massiva (Bulk Export) con filtraggio preliminare per data, tipologia di file o album.

### 8. Gestione Informazioni Personali (PIM)
- **Contatti (`AddressBook.sqlitedb`)**: visualizzazione anagrafica dettagliata ed esportazione conforme in vCard 3.0 (.vcf) e Microsoft Excel (.xlsx).
- **Note (`NoteStore.sqlite`)**: estrazione della gerarchia delle cartelle, snippet e decodifica dei messaggi Protobuf per il recupero di testo formattato completo e tabelle integrate.
- **Calendari (`Calendar.sqlitedb`)**: estrazione eventi, scadenze, partecipanti ed esportazione standard RFC 5545 iCalendar (.ics).
- **Promemoria**: supporto duale sia per lo schema moderno iOS 13+ (`Stores/Data-*.sqlite`) che per lo schema legacy integrato in CalendarItem.

### 9. Telecomunicazioni e Audio
- **Registro Chiamate (`CallHistory.storedata`)**: cronologia dettagliata con differenziazione chiamate in entrata, perse, effettuate, durata in secondi e supporto per chiamate VoIP (FaceTime, WhatsApp).
- **Segreteria Telefonica (`voicemail.db`)**: correlazione metadati con flussi audio AMR ed esportazione diretta.
- **Memo Vocali (`CloudRecordings.db`)**: metadati delle registrazioni e salvataggio dei file master AAC/M4A.

### 10. Navigazione Web (Safari)
- Analisi cronologica delle visite (`History.db`) con conteggio accessi e date secondo lo standard Cocoa Core Data (secondi da 2001-01-01).
- Ispezione di Preferiti ed Elenco di Lettura (`Bookmarks.db`) con esportazione in formato Netscape Bookmarks HTML compatibile con qualsiasi browser desktop.

### 11. Scanner di Indicatori di Compromissione Spyware (IOC)
- Modulo di audit per la rilevazione di sorveglianza mirata (Pegasus di NSO Group, Predator di Cytrox e domini di Command & Control noti).
- Scansione offline incrociata su SMS, chat WhatsApp e cronologia web Safari senza necessità di connessione di rete esterna.

### 12. Gestione Container Applicativi (.imazingapp)
- Ispezione dei metadati `Info.plist`, `iTunesMetadata.plist` e icone delle app installate.
- Generazione ed esportazione di archivi standard `.imazingapp` conformi alla specifica ZIP con preservazione dell'albero `Container/`.

### 13. Backup Extractor (Estrazione di Massa)
- Motore automatico di deserializzazione e salvataggio dell'intero contenuto del backup in cartelle strutturate sul file system host (Contatti, Messaggi, Note, Media, Calendari, Safari).

---

## Pipeline di Super-Risoluzione SOTA

IManCraft integra due motori dedicati di super-risoluzione neurale accelerati via hardware GPU Vulkan: uno specializzato per il restauro fotografico a 6 stadi (4× / 8×) e uno per la super-risoluzione dell'intero flusso video frame-by-frame (4× MP4 con audio AAC e filtro CAS).

### 1. Pipeline di Restauro Fotografico a 6 Stadi (UltraEnhance 4× / 8×)

Per il recupero di fotografie compresse, anteprime WhatsApp e asset archiviati su iCloud di cui persiste solo la miniatura (120–320 px), la pipeline esegue:

```
Sorgente (Miniatura / Fotografia compressa)
  │
  ├── Stadio 1: Pre-Denoise Bilaterale Edge-Preserving (Rayon multi-thread su CPU)
  │
  ├── Stadio 2: Super-Risoluzione Neurale Vulkan (Real-ESRGAN x4plus via GPU Compute Shaders)
  │             [Tiling automatico a blocchi da 256/400 px per prevenire VRAM OOM]
  │
  ├── Stadio 3: Fusione Cromatica YCbCr / Wavelet (Luminanza AI ad alta frequenza + crominanza fedele)
  │
  ├── Stadio 4: Recupero Dinamico dei Mezzitoni e Ombre (Compensazione curve di contrasto)
  │
  ├── Stadio 5: Contrast-Adaptive Sharpening (AMD CAS parallelo senza ringing o artefatti)
  │
  └── Stadio 6: Sintesi di Micro-Texture Organica Fotografica (Anti-plastica su alte frequenze)
  │
Risultato Finale (4× / 8× Ultra-Risoluzione JPEG)
```

- **Fattori di Scala**: Selezione istantanea tra 4× (Alta Risoluzione) e 8× (Ultra Gigapixel).
- **Cascata di Sicurezza 8×**: Quando la risoluzione al primo passaggio supera i 2048 px, il secondo passaggio 2× viene rifinito istantaneamente da sinc finestrato Lanczos3 e sharpening CAS in meno di 150 ms, prevenendo saturazioni hardware.
- **Tolleranza ai Guasti Hardware**: In assenza di GPU dedicata o in caso di errore di allocazione Vulkan (`VK_ERROR_OUT_OF_DEVICE_MEMORY`), la pipeline effettua un fallback trasparente sull'algoritmo geometrico puro Rust (Lanczos3 + CAS).
- **Clamp di Sicurezza Texture WGPU**: L'anteprima a schermo per il viewport egui viene limitata a 4096 px per garantire compatibilità con i limiti delle API grafiche DirectX/Vulkan, preservando l'immagine a risoluzione piena in memoria per il salvataggio su disco in qualità 100%.

### 2. Pipeline di Super-Risoluzione Video Neurale SOTA (4× MP4)

Concepita specificamente per l'upscaling integrale di videomessaggi (note video WhatsApp a 480×480 px), clip compresse e flussi registrati su dispositivi mobili, la pipeline video converte il file a piena risoluzione 4× (es. da 480×480 SD a 1920×1920 Full HD) preservando l'audio in perfetto sincronismo temporale:

```
File Video Sorgente (es. Videomessaggio WhatsApp 480×480 @ 30 fps)
  │
  ├── 1. Probing & Analisi Metadati: Rilevamento frame rate (FPS) e orientamento matriciale nativo
  │
  ├── 2. Estrazione Fotogrammi Lossless: De-compressione ad alta fedeltà di tutti i frame in directory temporanea isolata
  │
  ├── 3. Estrazione Flusso Audio: Copia diretta della traccia audio AAC originale senza ricodifica
  │
  ├── 4. Inferenza Neurale Batch GPU Vulkan: Esecuzione di Real-ESRGAN x4plus con tiling a 256 px a protezione della memoria VRAM
  │
  ├── 5. Filtraggio Acutance: Applicazione del filtro hardware Contrast-Adaptive Sharpening (AMD CAS a 0.5)
  │
  └── 6. Re-muxing Hardware & Sincronizzazione: Codifica H.264 tramite encoder hardware NVENC (CRF 18 / Preset P6) o libx264, con traccia audio AAC sincronizzata
  │
File Video Risultante (1920×1920 MP4 Full HD)
```

- **Integrazione UI Reattiva**: Attivabile direttamente dal visualizzatore a tutto schermo tramite pulsante dedicato "Migliora Video con AI (SOTA 4× MP4)". Durante l'elaborazione, una modale animata visualizza l'avanzamento senza bloccare l'interfaccia.
- **Gestione del Risultato**: Al termine, la barra superiore consente con un clic di aprire la cartella di destinazione in Esplora Risorse o avviare la riproduzione del video migliorato con il lettore predefinito di sistema.
- **Pulizia Automatica (RAII)**: Tutte le sequenze di frame intermedi e i file di lavoro temporanei vengono rimossi istantaneamente al termine o in caso di annullamento.

---

## Modulo Crittografico e Recupero Note Protette

IManCraft include un motore crittografico puro Rust multi-thread dedicato alla verifica di sicurezza e al recupero credenziali per le note protette memorizzate in `NoteStore.sqlite`:

- **Estrazione Robusta dei Parametri Crittografici**: Parser binario resilient a byte grezzi per l'estrazione di `CryptoSalt`, `CryptoIterationCount`, `CryptoVerifier` e `wrappedEncryptionKey`, immune a fallimenti di deserializzazione dei bplist complessi.
- **Verifica Immediata RFC 3394**: Derivazione chiavi via PBKDF2-HMAC-SHA256 (20.000 iterazioni conformi alle specifiche Apple) e verifica istantanea tramite l'integrità del vettore `0xA6A6A6A6A6A6A6A6` definito dallo standard RFC 3394 (zero falsi positivi matematici).
- **Modalità di Attacco Parallelo su CPU (Rayon)**:
  - *Attacco basato su Indizio (Hint-based)*: Generazione automatica di permutazioni (capitalizzazioni alternate, trasformazioni leet-speak, suffissi numerici e temporali).
  - *Attacco PIN*: Verifica esaustiva delle combinazioni numeriche a 4 e 6 cifre.
  - *Attacco Wordlist*: Ingestione di dizionari esterni (.txt) con throughput multi-core ad alta densità.
- **Decrittazione AES-GCM e Decompressione**: Decifratura del payload a 128 o 256 bit, decompressione del flusso zlib/gzip ed estrazione dei tag Protobuf per visualizzare il testo originario.
- **Sblocco a Cascata**: La derivazione della chiave di un account sblocca contestualmente tutte le note cifrate appartenenti al medesimo profilo nel backup.

---

## Interfaccia a Riga di Comando (CLI)

Il binario headless `imancraft-cli` consente l'integrazione di IManCraft all'interno di pipeline automatizzate, ambienti headless e procedure di triage rapido:

```bash
# 1. Rilevamento automatico di tutti i backup presenti sull'host
imancraft-cli list

# 2. Ispezione dettagliata dei metadati (inclusi backup protetti da password)
imancraft-cli info "C:\Backups\iPhone_00008110-000A648136D0A01E" --password "segreta123"

# 3. Ricerca full-text su tutte le sorgenti di dati
imancraft-cli search "C:\Backups\iPhone_00008110-000A648136D0A01E" "coordinate_bancarie"

# 4. Scansione indicatori di compromissione spyware (Pegasus / Predator)
imancraft-cli spyware-scan "C:\Backups\iPhone_00008110-000A648136D0A01E"

# 5. Estrazione massiva automatizzata (Backup Extractor)
imancraft-cli extract "C:\Backups\iPhone_00008110-000A648136D0A01E" -o "D:\Evidenze_Estraibili"
```

---

## Requisiti di Sistema

| Componente | Requisito Minimo | Note di Configurazione |
|---|---|---|
| **Sistema Operativo** | Windows 10 / 11 a 64 bit | Architettura `x86_64` |
| **FFmpeg & FFprobe** | Versione 6.0 o superiore (nel PATH) | Necessario per la transcodifica HEIC/HEIF e il demux video/audio |
| **GPU con supporto Vulkan** | Driver Vulkan 1.3 compatibili | Accelerazione hardware per la Super-Risoluzione SOTA Real-ESRGAN (NVIDIA, AMD, Intel). *In assenza di GPU, l'elaborazione passa automaticamente su CPU.* |
| **Backup iOS** | Da iTunes, Finder, Apple Devices o iMazing | Cartella con `Manifest.db`, `Info.plist` e `Status.plist` |

### Installazione di FFmpeg su Windows
Qualora FFmpeg non risulti configurato nel PATH di sistema, è possibile installarlo tramite il gestore di pacchetti nativo di Windows:

```powershell
winget install Gyan.FFmpeg
```

In alternativa, estrarre i binari `ffmpeg.exe` e `ffprobe.exe` direttamente nella directory contenente l'eseguibile `imancraft.exe`.

### Percorsi di Rilevamento Predefiniti dei Backup iOS
IManCraft scansiona automaticamente le seguenti posizioni di sistema:
- **Applicazione Dispositivi Apple / iTunes moderno**: `%USERPROFILE%\Apple\MobileSync\Backup`
- **iTunes classico**: `%APPDATA%\Apple Computer\MobileSync\Backup`
- **Directory arbitrarie**: Configurabili da menu (`File -> Aggiungi cartella di backup...` o `Ctrl + O`).

---

## Compilazione dai Sorgenti

### Prerequisiti di Build
- Toolchain Rust `stable-x86_64-pc-windows-gnu`.
- Linker `rust-lld` (configurato in `.cargo/config.toml`).
- Libreria di importazione SQLite: il repository include `lib/sqlite3.dll` e la relativa import library `lib/libsqlite3.dll.a`. La build script di Cargo copia automaticamente le librerie nei percorsi target.

### Procedura di Compilazione

```bash
# 1. Clonazione del repository
git clone https://github.com/SuicideBoy/iManCraft.git
cd iManCraft

# 2. Esecuzione della suite di test automatizzati (39 test unitari e di integrazione)
cargo test --workspace

# 3. Compilazione ottimizzata in modalità Release
cargo build --release --workspace
```

Gli eseguibili compilati saranno generati in:
- `target/x86_64-pc-windows-gnu/release/imancraft.exe` (Applicazione Desktop)
- `target/x86_64-pc-windows-gnu/release/imancraft-cli.exe` (Interfaccia CLI)

---

## Scorciatoie da Tastiera

| Combinazione | Contesto | Azione |
|---|---|---|
| `Ctrl + D` | Globale | Passa alla schermata **Discover** (Panoramica e azioni rapide) |
| `Ctrl + F` | Globale | Attiva la **Ricerca Globale** unificata |
| `Ctrl + E` | Globale | Apri lo strumento **Backup Extractor** (Esportazione guidata) |
| `Ctrl + O` | Globale | Apri dialogo per aggiungere una directory di backup esterna |
| `Ctrl + R` | Globale | Ricarica e riesegui la scansione delle cartelle di backup |
| `Ctrl + H` | Globale | Visualizza la cronologia delle operazioni e log di background |
| `Ctrl + ,` | Globale | Apri il pannello **Impostazioni** (Tema, accento di colore, scala UI) |
| `Esc` | Visualizzatore | Chiudi il visualizzatore a schermo intero o il modale attivo |
| `Freccia Sinistra / Destra` | Visualizzatore | Naviga all'elemento multimediale precedente / successivo |
| `Spazio` | Player | Avvia / Metti in pausa la riproduzione video o audio vocale |

---

## Struttura del Repository

```
iManCraft/
├── app/                  # Applicazione grafica nativa (egui 0.37 + wgpu)
│   ├── src/
│   │   ├── views/        # Moduli visuali per le 19 sezioni dell'interfaccia
│   │   ├── upscaler.rs   # Pipeline neurale SOTA UltraEnhance (4x / 8x)
│   │   ├── images.rs     # Gestore cache texture asincrona LIFO
│   │   ├── player.rs     # Riproduttore multimediale video/audio rodio + ffmpeg
│   │   ├── theme.rs      # Design system Craft (color tokens, font, layout)
│   │   └── main.rs       # Entry point della GUI
├── cli/                  # Interfaccia headless per scripting e automazione
├── crates/
│   └── core/             # Libreria core (imancraft-core) per parsing e crittografia
├── assets/               # Icone e risorse vettoriali
├── docs/                 # Documentazione tecnica e matrici di compatibilità
├── lib/                  # Dipendenze native e librerie di importazione SQLite
├── tools/                # Runtime opzionali per l'inferenza Real-ESRGAN Vulkan
├── dist/                 # Struttura di distribuzione e pacchettizzazione Windows
└── installer.iss         # Script Inno Setup per la creazione del pacchetto installer
```

---

## Licenza e Disclaimer Legale

Questo progetto è distribuito sotto i termini della licenza **MIT**. Consultare il file [LICENSE](LICENSE) per i termini completi.

*Disclaimer: IManCraft è un progetto open source indipendente sviluppato per scopi di ricerca scientifica, interoperabilità dei dati e analisi forense digitale. Apple, iPhone, iPad, iOS, iTunes, Finder e iCloud sono marchi registrati di Apple Inc. iMazing è un marchio registrato di DigiDNA SARL. IManCraft non è affiliato, sponsorizzato né approvato da alcuna delle suddette entità commerciali.*
