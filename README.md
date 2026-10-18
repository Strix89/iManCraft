# 📱 IManCraft

<p align="center">
  <strong>Client forense nativo per l'esplorazione, l'analisi e l'estrazione di backup iOS (iTunes / Finder / iMazing).</strong><br>
  <em>100% Rust, zero browser, zero Electron. Interfaccia grafica nativa in stile ArtCraft (egui + wgpu) con Super-Risoluzione AI accelerata da GPU.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust%202021-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Windows%20x64-blue.svg" alt="Windows x64">
  <img src="https://img.shields.io/badge/GUI-egui%200.37%20%2B%20wgpu-purple.svg" alt="egui">
  <img src="https://img.shields.io/badge/GPU-Vulkan%20Accelerated-green.svg" alt="Vulkan">
  <img src="https://img.shields.io/badge/License-MIT-brightgreen.svg" alt="License">
</p>

---

## 🌟 Panoramica

**IManCraft** è un'applicazione desktop Windows per l'esplorazione forense approfondita e l'estrazione dati dai backup di iPhone e iPad, siano essi standard o cifrati con password. 

Ispirato al linguaggio visivo della suite **ArtCraft** (antracite denso, accenti configurabili, chip monospazio ad alta densità informativa), IManCraft opera in modalità **completamente offline e in sola lettura**, garantendo l'integrità assoluta dei backup originali. La decifratura dei backup protetti avviene **esclusivamente in memoria RAM** tramite derivazione crittografica RFC 3394 / PBKDF2 / AES-CBC, senza mai scrivere dati in chiaro su disco.

---

## ⚡ Caratteristiche Principali

### 🔍 Esplorazione Forense Completa (19 Sezioni)
- **Discover Screen**: Hero Card con dettagli tecnici estesi (UDID, versione iOS, stato cifratura, bitmask DDNA), **barra di memoria segmentata** per quote disco (Foto, Video, App, Audio, Dati) e **griglia di Quick Actions** per la navigazione rapida.
- **WhatsApp Forensics**: Ricostruzione conversazioni a bolle con allegati, citazioni e messaggi di sistema. **Ricerca avanzata per intervallo di date e ore** con scorciatoie rapide. Galleria multimediale separata, registro chiamate ed esportazione chat in formato **TXT, CSV, XLSX (Excel)** e **HTML5 responsivo con impaginazione PDF/Stampa**.
- **Telegram iOS Forensics**: Rilevamento istanze app e App Group (`group.ph.telegra.Telegraph`), parsing delle preferenze plist (`CFAbsoluteTime` / Unix startup time, flag di indicizzazione e preset video). Analisi dei database Postbox (`db_sqlite`) e legacy (`tgdata.sqlite`), con gestione e spiegazione dettagliata dell'esclusione cloud (`NSURLIsExcludedFromBackupKey`).
- **Messaggi (iMessage / SMS)**: Visualizzazione chat con allegati multimediali, visualizzatore nativo ed esportazione sia in formato **Excel (XLSX)** che **HTML per stampa/salvataggio PDF**.
- **Libreria Foto & Video**: Esplorazione dell'intera libreria `Photos.sqlite`, inclusi gli elementi rimasti **solo su iCloud** (con recupero dell'anteprima e badge indicativo ☁), album, filtri dinamici, player video nativo frame-by-frame e **funzione di esportazione massiva (Bulk Export)**.
- **Contatti, Note, Calendari & Promemoria**:
  - Contatti con esportazione **vCard 3.0 (.vcf standard)** ed Excel.
  - Note con decodifica dei messaggi Protobuf per recuperare testo formattato completo e cartelle.
  - Calendari con esportazione **iCalendar (.ics standard RFC 5545)**.
  - Promemoria completi (iOS 13+ `Stores/Data-*.sqlite` e legacy) con priorità, scadenze e stato completamento.
- **Registro Chiamate, Segreteria & Memo Vocali**: Registro completo delle chiamate con durata e stato; ascolto e salvataggio diretto dei file audio nativi della segreteria (`.amr`) e dei memo vocali (`.m4a`).
- **Safari**: Navigazione cronologia e gestione completa dei **Preferiti ed Elenco di Lettura** con esportazione HTML standard Netscape Bookmarks.
- **Rilevamento Spyware IOC (Pegasus / Predator)**: Scanner offline per la ricerca di Indicatori di Compromissione (IOC) di spyware di sorveglianza mirata (Pegasus di NSO Group, Predator di Cytrox, C2 domains) su SMS, WhatsApp e cronologia web.
- **DDNA & Snapshot Incrementali Versions.db**: Parsing della bitmask a 32 bit `dataSets` di iMazing, navigazione visiva della timeline degli snapshot incrementali con calcolo differenze tra versioni.
- **Container App `.imazingapp`**: Ispezione ed esportazione dei container delle applicazioni in archivi standard `.imazingapp` (con `Container/`, `iTunesMetadata.plist` e icone).
- **Backup Extractor (Export All Data)**: Motore di estrazione guidata per salvare l'intero backup organizzato automaticamente in cartelle sul computer.

---

### ✨ Super-Risoluzione AI con Accelerazione GPU
IManCraft integra un modulo di miglioramento e ingrandimento 4× per foto, anteprime compresse e frame video:
- **Accelerazione Hardware GPU (Vulkan)**: Esecuzione di modelli neurali profondi (**Real-ESRGAN Vulkan**) direttamente sui compute shader della scheda video (NVIDIA GeForce, AMD Radeon, Intel) in circa 2 secondi.
- **Algoritmo Puro Rust (Lanczos3 + Contrast Adaptive Sharpening CAS)**: Fallback matematico con ricampionamento sinc finestrato e filtro dinamico CAS per recuperare nitidezza e micro-dettagli senza aloni o artefatti di ringing.
- **Interfaccia Craft**: Dialogo modale con indicazione GPU, barra animata pulsante, switcher istantaneo `[✨ 4× AI | Originale]` e salvataggio diretto in alta risoluzione.

---

### 💻 Strumento da Riga di Comando (`imancraft-cli`)
Oltre alla GUI nativa, il progetto include l'eseguibile headless `imancraft-cli`:
```bash
# Scansione dei backup sul computer
imancraft-cli list

# Informazioni dettagliate su un backup (anche con password)
imancraft-cli info "C:\Percorso\Al\Backup" --password "mia_password"

# Ricerca globale su tutti i database
imancraft-cli search "C:\Percorso\Al\Backup" "parola_chiave"

# Scansione IOC spyware (Pegasus / Predator)
imancraft-cli spyware-scan "C:\Percorso\Al\Backup"

# Estrazione completa in cartelle (Backup Extractor)
imancraft-cli extract "C:\Percorso\Al\Backup" -o "C:\Destinazione"
```

---

## 📋 Prerequisiti

| Componente | Requisito | A cosa serve |
|---|---|---|
| **Sistema Operativo** | Windows 10 / 11 a 64-bit | Sistema operativo supportato |
| **FFmpeg & FFprobe** *(Consigliato)* | Presente nel PATH di sistema | Decodifica delle foto Apple HEIC/HEIF, estrazione miniature e riproduzione video/audio vocali |
| **GPU con supporto Vulkan** *(Opzionale)* | NVIDIA GeForce (GTX/RTX), AMD Radeon, Intel Arc | Accelerazione hardware per la Super-Risoluzione AI 4×. *Se assente, l'app usa in automatico la CPU senza blocchi.* |
| **Backup iOS** | Da iTunes, Finder, Apple Devices o iMazing | Cartella di backup standard o personalizzata |

### 💡 Installazione Rapida di FFmpeg
Se non hai `ffmpeg` installato, puoi aggiungerlo in un solo secondo aprendo PowerShell ed eseguendo:
```powershell
winget install Gyan.FFmpeg
```
Oppure scaricando la versione statica da [gyan.dev/ffmpeg/builds](https://www.gyan.dev/ffmpeg/builds/) ed estraendo `ffmpeg.exe` e `ffprobe.exe` nel PATH di Windows o nella stessa cartella di `imancraft.exe`.

### 📂 Dove si trovano i backup iOS su Windows?
IManCraft scansiona in automatico le cartelle predefinite di Apple:
- **Apple Devices / iTunes moderno**: `%USERPROFILE%\Apple\MobileSync\Backup`
- **iTunes classico**: `%APPDATA%\Apple Computer\MobileSync\Backup`
- **Cartelle esterne (Hard Disk / SSD USB)**: Puoi aggiungere qualsiasi cartella personalizzata tramite **File → Aggiungi cartella di backup…** (`Ctrl+O`).

---

## 🚀 Installazione

### Metodo 1: Pacchetto Portabile Precompilato (Consigliato)
1. Scarica l'ultima release `IManCraft-vX.Y.Z-win64.zip` dalla sezione [Releases](https://github.com/SuicideBoy/iManCraft/releases).
2. Estrai il contenuto del file ZIP in una cartella a tuo piacimento (es. `C:\Programmi\IManCraft\` o sul Desktop).
3. Avvia l'applicazione facendo doppio clic su `imancraft.exe` oppure sullo script rapido `Avvia IManCraft.bat`.
   - *Zero installazione nel registro di Windows: l'app è completamente portabile.*
   - *Assicurati che `sqlite3.dll` rimanga sempre nella stessa cartella dell'eseguibile.*

### Metodo 2: Compilazione dai Sorgenti (per sviluppatori)
```bash
# 1. Clona il repository
git clone https://github.com/SuicideBoy/iManCraft.git
cd iManCraft

# 2. Esegui i test di verifica (20 test)
cargo test --workspace

# 3. Compila gli eseguibili ottimizzati in modalità Release
cargo build --release --workspace
```
Gli eseguibili saranno generati in `target/release/` (`imancraft.exe` e `imancraft-cli.exe`).

---

## 📖 Guida all'Uso Passo-Passo

### 1. Selezione e Caricamento del Backup
All'avvio, IManCraft scansiona automaticamente le cartelle di sistema e presenta i backup rilevati nella schermata **Dispositivi**:
- Fai clic sulla scheda del dispositivo desiderato per caricarlo come backup attivo.
- Se il backup si trova su un hard disk esterno o in un percorso differente, premi **`Ctrl + O`** (o menu *File → Aggiungi cartella di backup…*) e seleziona la cartella contenente il backup.

### 2. Sblocco dei Backup Cifrati con Password
Se il backup è protetto da cifratura iOS:
- Nella parte superiore apparirà un avviso con un campo di testo protetto per la password.
- Inserisci la password impostata su iOS e fai clic su **Sblocca**.
- *Garantito al 100% in RAM:* Le chiavi crittografiche e i database vengono decifrati **esclusivamente in memoria volatile**. Nessun file decifrato viene salvato su disco.

### 3. Navigazione tra le Sezioni (Rail a Sinistra)
Usa il menu laterale a scomparsa per consultare i tuoi dati:
- **Discover (`Ctrl + D`)**: Panoramica con barra segmentata dello spazio su disco (Foto, App, Audio, Video, Altro), stato del dispositivo e 8 pulsanti di azione rapida.
- **WhatsApp**: Esplora le chat, ascolta i messaggi vocali con il player audio integrato, filtra i messaggi per **intervallo esatto di date e ore** e visualizza la galleria multimediale completa.
- **Telegram Forensics**: Ispezione forense delle preferenze, startup time, timestamp della cache e database Postbox/legacy.
- **Foto & Video**: Griglia virtualizzata di tutte le foto e video. Gli elementi memorizzati **solo su iCloud** vengono contrassegnati con l'icona ☁ e mostrano l'anteprima memorizzata localmente. Fai doppio clic su una foto per aprire il visualizzatore a tutto schermo con zoom fluido e navigazione con frecce.
- **Cerca (`Ctrl + F`)**: Ricerca globale istantanea simultanea su Contatti, Note, Calendari, Promemoria, chat WhatsApp e messaggi SMS.
- **Sicurezza**: Esegui con un clic una scansione forense contro gli IOC (indicatori di compromissione) degli spyware Pegasus e Predator.

### 4. Come Usare la Super-Risoluzione AI GPU 4×
1. Apri una qualsiasi foto, anteprima compressa di WhatsApp o fermo immagine video a tutto schermo nel visualizzatore.
2. Nella barra superiore fai clic sul pulsante **`✨ Migliora con AI (GPU 4×)`**.
3. Apparirà il pannello modale ArtCraft con barra di progresso pulsante mentre la scheda video (NVIDIA GeForce / AMD Radeon via Vulkan) ricostruisce i dettagli ad alta frequenza (in circa 2 secondi).
4. Al termine, usa lo switcher interattivo **`[✨ 4× AI (Migliorato) | Originale]`** per confrontare istantaneamente la qualità prima e dopo.
5. Fai clic su **`⬇ Salva 4× JPEG…`** per esportare l'immagine migliorata a piena risoluzione.

### 5. Esportazione dei Dati
- **Excel (.xlsx)**: Disponibile in Messaggi, WhatsApp, Chiamate, Contatti, Calendari e Promemoria per esportare fogli di calcolo professionali.
- **Stampa / PDF (.html)**: In Messaggi e WhatsApp puoi esportare un documento HTML responsive con layout a bolle conforme alla stampa A4 o salvataggio nativo in PDF dal browser.
- **vCard (.vcf) & iCalendar (.ics)**: Esportazione conforme agli standard RFC per importare contatti e calendari su Outlook, Google o un nuovo iPhone.
- **Backup Extractor (`Ctrl + E`)**: Dalla sezione *Strumenti*, esporta l'intero backup organizzato automaticamente in cartelle (Contatti, Note, Calendario, ecc.) con un solo clic.

---

## ⌨️ Scorciatoie da Tastiera

| Scorciatoia | Azione |
|---|---|
| `Ctrl + D` | Vai alla schermata **Discover** (Panoramica e azioni rapide) |
| `Ctrl + F` | Apri la **Ricerca Globale** unificata |
| `Ctrl + E` | Apri lo strumento **Backup Extractor** (Esportazione di massa) |
| `Ctrl + O` | Aggiungi una cartella di backup personalizzata o esterna |
| `Ctrl + R` | Ricarica e ri-scansiona i backup su disco |
| `Ctrl + H` | Mostra la **Cronologia Operazioni** e log in background |
| `Ctrl + ,` | Apri le **Impostazioni** (Tema, accento di colore, zoom) |
| `Esc` | Chiudi il visualizzatore foto/video a tutto schermo |
| `← / →` | Foto precedente / successiva nel visualizzatore |
| `Spazio` | Play / Pausa riproduzione video o audio |

---

## 📁 Struttura del Progetto

```
iManCraft/
├── app/                  # Interfaccia grafica nativa egui + wgpu (Design ArtCraft)
│   ├── src/
│   │   ├── views/        # Le 19 sezioni visuali dell'applicazione
│   │   ├── upscaler.rs   # Motore di Super-Risoluzione AI GPU & Lanczos3+CAS
│   │   ├── images.rs     # Cache texture asincrona con priorità LIFO
│   │   ├── player.rs     # Riproduttore video e audio nativo
│   │   ├── theme.rs      # Design system Craft (palette, densità, token grafici)
│   │   └── main.rs       # Entrypoint della GUI
├── cli/                  # CLI standalone headless (imancraft-cli)
├── crates/
│   └── core/             # Libreria core di analisi forense, crittografia e database
├── assets/               # Icone e risorse dell'applicazione
├── docs/                 # Documentazione tecnica e matrici di reverse engineering
├── lib/                  # Dipendenze native SQLite per Windows
├── tools/                # Motori opzionali per l'upscaling neurale
├── package.ps1           # Script di automazione packaging e checksum SHA-256
└── installer.iss         # Script Inno Setup per l'installer Windows
```

---

## ⚖️ Licenza & Disclaimer

Questo software è distribuito sotto licenza **MIT** (vedi [LICENSE](LICENSE)).

*Disclaimer: IManCraft è un progetto open source indipendente sviluppato per scopi di analisi forense, backup personale e interoperabilità dei dati. iPhone, iPad, iOS, iTunes e Apple sono marchi registrati di Apple Inc. iMazing è un marchio registrato di DigiDNA SARL. IManCraft non è affiliato con né approvato da alcuna delle suddette entità.*
