//! IManCraft CLI — strumento da riga di comando per l'analisi forense,
//! l'ispezione e l'esportazione automatizzata dei backup iOS.

use imancraft_core::{
    calendars, data, default_roots, discover, export, extractor,
    imazing_app, imazing_versions, reminders, search, spyware, telegram, whatsapp,
    Backup,
};
use std::path::PathBuf;
use std::sync::Arc;

type CliResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return;
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];

    let res = match cmd {
        "list" | "list-backups" => cmd_list(rest),
        "info" => cmd_info(rest),
        "search" => cmd_search(rest),
        "spyware-scan" | "scan" => cmd_spyware_scan(rest),
        "telegram" => cmd_telegram(rest),
        "extract" => cmd_extract(rest),
        "export-messages" => cmd_export_messages(rest),
        "export-whatsapp" => cmd_export_whatsapp(rest),
        "export-contacts" => cmd_export_contacts(rest),
        "export-calendar" => cmd_export_calendar(rest),
        "export-reminders" => cmd_export_reminders(rest),
        "export-app" => cmd_export_app(rest),
        "snapshots" => cmd_snapshots(rest),
        "unlock-notes" => cmd_unlock_notes(rest),
        other => {
            eprintln!("Comando sconosciuto: '{other}'. Digita 'imancraft-cli help' per la guida.");
            std::process::exit(1);
        }
    };

    if let Err(e) = res {
        eprintln!("\n❌ Errore: {e}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        r#"IManCraft CLI v{} — Strumento da riga di comando per backup iOS

USO:
    imancraft-cli <COMANDO> [OPZIONI]

COMANDI DI ISPEZIONE:
    list [CARTELLA...]
        Elenca tutti i backup trovati nelle cartelle standard di sistema o nei percorsi specificati.

    info <BACKUP_PATH> [--password <PWD>]
        Mostra le informazioni dettagliate del dispositivo, stato crittografia, quote disco per categoria
        e metadati DDNABackup / Versions.db.

    search <BACKUP_PATH> <QUERY> [--password <PWD>]
        Esegue una ricerca globale unificata simultanea su Contatti, Note, Calendario, Promemoria, WhatsApp e Messaggi.

    spyware-scan <BACKUP_PATH> [--password <PWD>]
        Effettua la scansione forense offline alla ricerca di minacce spyware note (Pegasus, Predator, C2).

    telegram <BACKUP_PATH> [--password <PWD>]
        Ispezione forense Telegram per iOS (rilevamento app, gruppi condivisi, preferenze e policy esclusione).

    snapshots <BACKUP_PATH>
        Elenca tutti gli snapshot incrementali memorizzati nel database Versions.db di iMazing.

COMANDI DI ESPORTAZIONE:
    extract <BACKUP_PATH> --output <DIR> [--password <PWD>]
        Estrae l'intero backup in cartelle ordinate su disco (Contatti, Note, Calendario, Promemoria, Chiamate, Safari, Voicemail, Memo vocali).

    export-messages <BACKUP_PATH> --output <FILE> [--format <html|xlsx>] [--password <PWD>]
        Esporta la cronologia SMS / iMessage in HTML (pronto per stampa/PDF) o foglio Excel (.xlsx).

    export-whatsapp <BACKUP_PATH> --output <FILE> [--format <html|xlsx>] [--chat-id <ID>] [--password <PWD>]
        Esporta le conversazioni WhatsApp in HTML (pronto per stampa/PDF) o foglio Excel (.xlsx).

    export-contacts <BACKUP_PATH> --output <FILE> [--format <vcf|xlsx>] [--password <PWD>]
        Esporta i contatti della rubrica in vCard 3.0 (.vcf) o foglio Excel (.xlsx).

    export-calendar <BACKUP_PATH> --output <FILE> [--format <ics|xlsx>] [--password <PWD>]
        Esporta gli eventi di calendario in iCalendar (.ics) o foglio Excel (.xlsx).

    export-reminders <BACKUP_PATH> --output <FILE.xlsx> [--password <PWD>]
        Esporta l'elenco completo dei promemoria con priorità e scadenze in Excel (.xlsx).

    export-app <BACKUP_PATH> <BUNDLE_ID> --output <FILE.imazingapp> [--password <PWD>]
        Estrae il container di sandbox di una singola app nel formato compresso standard .imazingapp.

    unlock-notes <BACKUP_PATH> <PASSWORD_OR_PIN> [--output <FILE.txt>]
        Decifra e mostra a video tutte le note protette da password/PIN del backup, con opzione di salvataggio su file.

ESEMPI:
    imancraft-cli list
    imancraft-cli info "C:\Backups\00008110-000A648136D0A01E"
    imancraft-cli unlock-notes "C:\Backups\00008110-000A648136D0A01E" 12345678 --output note.txt
    imancraft-cli search "C:\Backups\iPhone14" "Mario Rossi"
    imancraft-cli extract "C:\Backups\iPhone14" --output "C:\Export_iPhone"
    imancraft-cli export-contacts "C:\Backups\iPhone14" --output "rubrica.vcf"
"#,
        env!("CARGO_PKG_VERSION")
    );
}

// ---------------------------------------------------------------- Commando list

fn cmd_list(args: &[String]) -> CliResult {
    let mut roots = default_roots();
    for a in args {
        if !a.starts_with('-') {
            roots.push(PathBuf::from(a));
        }
    }

    println!("Scansione dei backup in corso...");
    let dirs = discover(&roots);
    if dirs.is_empty() {
        println!("Nessun backup trovato nelle posizioni cercate.");
        return Ok(());
    }

    println!("\nTrovati {} backup:", dirs.len());
    println!("{:<36} {:<22} {:<10} {:<14} {:<8} {:<10}", "UDID", "Dispositivo", "iOS", "Data", "Cifrato", "Spazio");
    println!("{}", "-".repeat(106));

    for d in dirs {
        let b = Backup::open(d);
        let s = &b.summary;
        let date_short = s.date.get(..10).unwrap_or(&s.date);
        let enc_str = if b.encrypted { "Sì" } else { "No" };
        println!(
            "{:<36} {:<22} {:<10} {:<14} {:<8} {:<10}",
            short(&s.udid, 35),
            short(&format!("{} ({})", s.name, s.model), 21),
            s.ios,
            date_short,
            enc_str,
            fmt_size(s.size)
        );
    }
    println!();
    Ok(())
}

// ---------------------------------------------------------------- Commando info

fn cmd_info(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let b = open_and_unlock(&b_path, pwd.as_deref())?;

    println!("\n══════════════════════════════════════════════════════════════");
    println!(" INFORMAZIONI DISPOSITIVO & BACKUP");
    println!("══════════════════════════════════════════════════════════════");
    println!(" Nome dispositivo:    {}", b.summary.name);
    println!(" Modello:             {}", b.summary.model);
    println!(" Versione iOS:        {}", b.summary.ios);
    println!(" Data backup:         {}", b.summary.date);
    println!(" UDID:                {}", b.summary.udid);
    println!(" Cartella:            {}", b.dir.display());
    println!(" Spazio su disco:     {}", fmt_size(b.summary.size));
    println!(" Stato cifratura:     {}", if b.encrypted { "Protetto da password (cifrato)" } else { "In chiaro (non cifrato)" });

    if let Some(ddna) = b.ddna() {
        println!("\n── Metadati DDNABackup (iMazing) ─────────────────────────────");
        if !ddna.apple_id.is_empty() {
            println!(" Apple ID:            {}", ddna.apple_id);
        }
        if !ddna.model_details.is_empty() {
            println!(" Dettagli modello:    {}", ddna.model_details);
        }
        let active_ds = ddna.flags.active_datasets();
        println!(" Dataset abilitati:   {} categorie ({:?})", active_ds.len(), active_ds);

        if ddna.total_disk_usage > 0 {
            println!("\n── Ripartizione Memoria Dispositivo ──────────────────────────");
            println!(" Spazio totale DDNA:  {}", fmt_size(ddna.total_disk_usage));
            println!("   - Foto:            {}", fmt_size(ddna.photos_disk_usage));
            println!("   - App:             {}", fmt_size(ddna.apps_disk_usage));
            println!("   - Audio:           {}", fmt_size(ddna.audio_disk_usage));
            println!("   - Video:           {}", fmt_size(ddna.video_disk_usage));
        }
    }

    if let Some(vdb) = imazing_versions::find_versions_db(&b.dir) {
        println!("\n── Archivio Snapshot (Versions.db) ───────────────────────────");
        println!(" Database Versions:   {}", vdb.display());
        if let Ok(conn) = imazing_versions::open_versions_db(&vdb) {
            if let Ok(props) = imazing_versions::load_properties(&conn) {
                println!(" Schema archiviazione: v{}", props.archiving_sys_version);
            }
            if let Ok(snaps) = imazing_versions::list_snapshots(&conn) {
                println!(" Snapshot presenti:   {}", snaps.len());
            }
        }
    }

    println!("══════════════════════════════════════════════════════════════\n");
    Ok(())
}

// ---------------------------------------------------------------- Commando search

fn cmd_search(args: &[String]) -> CliResult {
    let (b_path, pwd, rest) = parse_backup_and_common(args)?;
    let query = rest.first().ok_or("Manca il termine di ricerca. Uso: search <backup_path> <query>")?;

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    println!("Ricerca di \"{}\" nel backup...", query);

    b.with(|cx| {
        let results = search::global_search(cx, query)?;
        if results.is_empty() {
            println!("Nessun risultato trovato.");
            return Ok(());
        }

        println!("\nTrovate {} corrispondenze:\n", results.len());
        for (i, r) in results.iter().enumerate() {
            let dt = r.date.map(|t| format!(" ({})", fmt_date(t))).unwrap_or_default();
            println!("[{}] {:<12} : {}{}", i + 1, r.category, r.title, dt);
            if !r.subtitle.is_empty() {
                println!("     └─ {}", r.subtitle);
            }
        }
        println!();
        Ok(())
    })
}

// ---------------------------------------------------------------- Commando spyware-scan

fn cmd_spyware_scan(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let b = open_and_unlock(&b_path, pwd.as_deref())?;

    println!("Avvio scansione offline degli indicatori di compromissione (IOC Pegasus / Predator)...");
    b.with(|cx| {
        let findings = spyware::scan_spyware(cx)?;
        println!("\n══════════════════════════════════════════════════════════════");
        println!(" REPORT FORENSE RILEVAMENTO SPYWARE");
        println!("══════════════════════════════════════════════════════════════");

        if findings.is_empty() {
            println!("✓ Nessuna minaccia o indicatore di compromissione rilevato.");
            println!("  I database di messaggi, chat WhatsApp e cronologia Safari sono puliti.");
        } else {
            println!("⚠ RILEVATI {} INDICATORI SOSPETTI:", findings.len());
            for (i, f) in findings.iter().enumerate() {
                println!("\n[Minaccia #{}]", i + 1);
                println!("  Origine:    {}", f.source);
                println!("  Minaccia:   {}", f.threat);
                println!("  Gravità:    {}", f.severity);
                println!("  Indicatore: {}", f.indicator);
                println!("  Dettagli:   {}", f.details);
            }
        }
        println!("══════════════════════════════════════════════════════════════\n");
        Ok(())
    })
}

// ---------------------------------------------------------------- Commando telegram

fn cmd_telegram(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let b = open_and_unlock(&b_path, pwd.as_deref())?;

    println!("Analisi forense Telegram in corso...");
    b.with(|cx| {
        let status = telegram::status(cx)?;

        match status {
            telegram::TelegramStatus::NotInstalled => {
                println!("\nTelegram non risulta installato in questo dispositivo/backup.");
            }
            telegram::TelegramStatus::InstalledExcludedFromBackup(meta) => {
                println!("\n══════════════════════════════════════════════════════════════");
                println!(" TELEGRAM iOS — ANALISI FORENSE");
                println!("══════════════════════════════════════════════════════════════");
                println!(" Stato:                    Installato (Dati chat esclusi da backup)");
                println!(" File totali nel backup:   {}", meta.app_files_count);
                println!(
                    " Container App Group:      {}",
                    if meta.has_group_container {
                        "Presente (group.ph.telegra.Telegraph)"
                    } else {
                        "Assente"
                    }
                );
                if let Some(t) = meta.startup_time {
                    println!(" Ultimo avvio (CKStartup): {}", fmt_date(t as f64));
                }
                if let Some(t) = meta.cache_index_timestamp {
                    println!(" Timestamp indice cache:   {}", fmt_date(t as f64));
                }
                println!(
                    " Raggruppamento media:     {}",
                    if meta.media_grouping_disabled { "Disabilitato" } else { "Abilitato" }
                );
                if let Some(p) = meta.preferred_video_preset {
                    println!(" Preset video preferito:   {}", p);
                }
                if !meta.databases_found.is_empty() {
                    println!("\n Database / indici individuati nel backup ({}):", meta.databases_found.len());
                    for db in &meta.databases_found {
                        println!("   • {}", db);
                    }
                }
                println!("\n⚠️  NOTA FORENSE:");
                println!("   Telegram per iOS applica la policy 'NSURLIsExcludedFromBackupKey = true'");
                println!("   alla cartella 'postbox/db/db_sqlite'. I messaggi cloud non vengono");
                println!("   inclusi nei backup locali standard di Apple per preservare lo spazio.");
                println!("══════════════════════════════════════════════════════════════\n");
            }
            telegram::TelegramStatus::Available { meta, chats } => {
                println!("\n══════════════════════════════════════════════════════════════");
                println!(" TELEGRAM iOS — CHAT ESTRATTE ({})", chats.len());
                println!("══════════════════════════════════════════════════════════════");
                println!(" File app totali: {}", meta.app_files_count);
                println!("{:<12} {:<32} {:<15} {:<10}", "ID", "Titolo", "Username", "Messaggi");
                println!("{}", "-".repeat(72));
                for c in chats {
                    println!(
                        "{:<12} {:<32} {:<15} {:<10}",
                        c.id,
                        short(&c.title, 30),
                        c.username.as_deref().unwrap_or("-"),
                        c.message_count
                    );
                }
                println!("══════════════════════════════════════════════════════════════\n");
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------- Commando extract

fn cmd_extract(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_dir = get_opt_arg(args, "--output").ok_or("Specifica la cartella di output con --output <DIR>")?;
    let b = open_and_unlock(&b_path, pwd.as_deref())?;

    let out_path = PathBuf::from(out_dir);
    println!("Avvio esportazione completa in: {}", out_path.display());

    b.with(|cx| {
        let summary = extractor::export_all_data(cx, &out_path, |step| {
            println!("  [→] {step}");
        })?;

        println!("\nEsportazione completata con successo!");
        println!("Riepilogo elementi estratti:");
        println!("  - Contatti:         {} (vCard + Excel)", summary.contacts_count);
        println!("  - Note:             {} file di testo", summary.notes_count);
        println!("  - Chiamate:         {} registrazioni (Excel)", summary.calls_count);
        println!("  - Eventi Calendario:{} (iCalendar + Excel)", summary.calendar_events_count);
        println!("  - Promemoria:       {} (Excel)", summary.reminders_count);
        println!("  - Messaggi Segreteria: {} (.amr)", summary.voicemail_count);
        println!("  - Memo Vocali:      {} (.m4a)", summary.voice_memos_count);
        println!("  - Preferiti Safari: {} (.html)", summary.bookmarks_count);
        println!();
        Ok(())
    })
}

// ---------------------------------------------------------------- Esportazioni specifiche

fn cmd_export_messages(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE>")?;
    let fmt_arg = get_opt_arg(args, "--format").unwrap_or_else(|| {
        if out_file.ends_with(".xlsx") { "xlsx".into() } else { "html".into() }
    });

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        println!("Caricamento conversazioni SMS/iMessage...");
        let chats = data::chats(cx)?;
        let mut all_msgs = Vec::new();
        for ch in &chats {
            if let Ok(msgs) = data::chat(cx, ch.id) {
                all_msgs.extend(msgs);
            }
        }
        all_msgs.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));

        if fmt_arg.to_lowercase() == "xlsx" {
            export::export_messages_xlsx("SMS & iMessage", &all_msgs, &dest)?;
        } else {
            export::export_messages_html("SMS & iMessage - Esportazione", &all_msgs, &dest)?;
        }

        println!("Esportati {} messaggi in {}", all_msgs.len(), dest.display());
        Ok(())
    })
}

fn cmd_export_whatsapp(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE>")?;
    let chat_id_opt: Option<i64> = get_opt_arg(args, "--chat-id").and_then(|s| s.parse().ok());
    let fmt_arg = get_opt_arg(args, "--format").unwrap_or_else(|| {
        if out_file.ends_with(".xlsx") { "xlsx".into() } else { "html".into() }
    });

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        let chats = whatsapp::chats(cx)?;
        if chats.is_empty() {
            println!("Nessuna chat WhatsApp trovata nel backup.");
            return Ok(());
        }

        let target_chat = if let Some(cid) = chat_id_opt {
            chats.into_iter().find(|c| c.id == cid).ok_or("Chat ID non trovata")?
        } else {
            chats[0].clone()
        };

        println!("Esportazione chat: {} (ID {})", target_chat.title, target_chat.id);
        let filter = whatsapp::Filter::default();

        if fmt_arg.to_lowercase() == "xlsx" {
            export::export_whatsapp_xlsx(cx, target_chat.id, &filter, 0, &dest)?;
        } else {
            export::export_whatsapp_html(&target_chat.title, cx, target_chat.id, &filter, 0, &dest)?;
        }

        println!("Chat esportata con successo in {}", dest.display());
        Ok(())
    })
}

fn cmd_export_contacts(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE>")?;
    let fmt_arg = get_opt_arg(args, "--format").unwrap_or_else(|| {
        if out_file.ends_with(".xlsx") { "xlsx".into() } else { "vcf".into() }
    });

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        let contacts_list = data::contacts(cx)?;
        if fmt_arg.to_lowercase() == "xlsx" {
            export::export_contacts_xlsx(&contacts_list, &dest)?;
        } else {
            let vcf = export::export_contacts_vcf(&contacts_list);
            std::fs::write(&dest, vcf)?;
        }
        println!("Esportati {} contatti in {}", contacts_list.len(), dest.display());
        Ok(())
    })
}

fn cmd_export_calendar(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE>")?;
    let fmt_arg = get_opt_arg(args, "--format").unwrap_or_else(|| {
        if out_file.ends_with(".xlsx") { "xlsx".into() } else { "ics".into() }
    });

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        let events = calendars::events(cx)?;
        if fmt_arg.to_lowercase() == "xlsx" {
            export::export_calendar_xlsx(&events, &dest)?;
        } else {
            let ics = calendars::export_ics(&events);
            std::fs::write(&dest, ics)?;
        }
        println!("Esportati {} eventi di calendario in {}", events.len(), dest.display());
        Ok(())
    })
}

fn cmd_export_reminders(args: &[String]) -> CliResult {
    let (b_path, pwd, _) = parse_backup_and_common(args)?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE.xlsx>")?;

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        let rems = reminders::reminders(cx)?;
        export::export_reminders_xlsx(&rems, &dest)?;
        println!("Esportati {} promemoria in {}", rems.len(), dest.display());
        Ok(())
    })
}

fn cmd_export_app(args: &[String]) -> CliResult {
    let (b_path, pwd, rest) = parse_backup_and_common(args)?;
    let bundle_id = rest.first().ok_or("Specifica il bundle ID dell'app. Uso: export-app <backup_path> <bundle_id> --output <FILE.imazingapp>")?;
    let out_file = get_opt_arg(args, "--output").ok_or("Specifica il file di destinazione con --output <FILE.imazingapp>")?;

    let b = open_and_unlock(&b_path, pwd.as_deref())?;
    let dest = PathBuf::from(&out_file);

    b.with(|cx| {
        println!("Esportazione container sandbox per '{}'...", bundle_id);
        let count = imazing_app::export_imazing_app(cx, bundle_id, &dest)?;
        println!("Creato archivio .imazingapp con {} file: {}", count, dest.display());
        Ok(())
    })
}

fn cmd_snapshots(args: &[String]) -> CliResult {
    let (b_path, _, _) = parse_backup_and_common(args)?;
    let vdb_path = imazing_versions::find_versions_db(&PathBuf::from(&b_path))
        .ok_or("Nessun database Versions.db di iMazing trovato in questo percorso.")?;

    let conn = imazing_versions::open_versions_db(&vdb_path)?;
    let snaps = imazing_versions::list_snapshots(&conn)?;

    println!("\nSnapshot registrati in {}:", vdb_path.display());
    println!("{:<4} {:<18} {:<10} {:<24} {:<12} {:<10}", "ID", "Data", "iOS", "Motivo", "Stato", "Dimensione");
    println!("{}", "-".repeat(84));

    for s in snaps {
        println!(
            "{:<4} {:<18} {:<10} {:<24} {:<12} {:<10}",
            s.id,
            fmt_date(s.timestamp as f64),
            s.ios_version,
            s.reason_label(),
            s.status_label(),
            fmt_size(s.original_size)
        );
    }
    println!();
    Ok(())
}

fn cmd_unlock_notes(args: &[String]) -> CliResult {
    let (b_path, pwd_opt, rest) = parse_backup_and_common(args)?;
    let password = rest.first().cloned().or(pwd_opt).ok_or("Specifica la password o PIN. Uso: unlock-notes <backup_path> <password> [--output <file.txt>]")?;
    let out_file = get_opt_arg(args, "--output");

    let b = Backup::open(PathBuf::from(&b_path));
    println!("Caricamento database NoteStore.sqlite da {}...", b.dir.display());
    let (verifier, protected_notes) = b.notes_crypto_context()?;

    if protected_notes.is_empty() {
        println!("Nessuna nota protetta trovata nel backup.");
        return Ok(());
    }

    println!("Trovate {} note protette. Tentativo di sblocco con '{}'...\n", protected_notes.len(), password);
    let mut decrypted_count = 0;
    let mut full_output = String::new();

    for pn in &protected_notes {
        match imancraft_core::notes_recovery::unlock_note(pn, verifier.as_ref(), &password) {
            Ok(text) => {
                decrypted_count += 1;
                let header = format!("=======================================================\n📄 NOTA #{}: {}\nCartella: {}\n=======================================================\n", pn.note_pk, pn.title, pn.folder);
                println!("{header}{text}\n");
                full_output.push_str(&header);
                full_output.push_str(&text);
                full_output.push_str("\n\n");
            }
            Err(e) => {
                println!("❌ Errore sblocco nota #{}: {e}", pn.note_pk);
            }
        }
    }

    if decrypted_count > 0 {
        println!("✓ Decifrate con successo {decrypted_count}/{} note protette!", protected_notes.len());
        if let Some(dest) = out_file {
            std::fs::write(&dest, &full_output)?;
            println!("Note salvate su file: {dest}");
        }
    } else {
        println!("❌ Impossibile decifrare le note con la chiave specificata.");
    }

    Ok(())
}

// ---------------------------------------------------------------- Helper

fn parse_backup_and_common(args: &[String]) -> Result<(String, Option<String>, Vec<String>), Box<dyn std::error::Error + Send + Sync>> {
    let pwd = get_opt_arg(args, "--password");
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        positional.push(args[i].clone());
        i += 1;
    }

    let b_path = positional.first().ok_or("Percorso del backup non specificato.")?.clone();
    let rest = positional.into_iter().skip(1).collect();
    Ok((b_path, pwd, rest))
}

fn get_opt_arg(args: &[String], flag: &str) -> Option<String> {
    for (i, a) in args.iter().enumerate() {
        if a == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

fn open_and_unlock(path: &str, password: Option<&str>) -> CliResult<Arc<Backup>> {
    let b = Arc::new(Backup::open(PathBuf::from(path)));
    if b.encrypted {
        if let Some(pwd) = password {
            println!("Sblocco del backup cifrato in memoria...");
            b.unlock(pwd)?;
            println!("Backup sbloccato con successo.");
        } else {
            return Err("Il backup è protetto da cifratura. Specifica la password con --password <tua_password>.".into());
        }
    }
    Ok(b)
}

fn fmt_size(n: u64) -> String {
    let mut v = n as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", units[i]) }
}

fn fmt_date(t: f64) -> String {
    if t <= 0.0 || t > 4_102_444_800.0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(t as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>()) }
}
