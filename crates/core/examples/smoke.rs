use imancraft_core::{calendars, data, discover, recordings, safari_bookmarks, spyware, voicemail, whatsapp, Backup};

use std::sync::Arc;
use std::time::Instant;

fn main() {
    let roots: Vec<_> = std::env::args().skip(1).map(Into::into).collect();
    for dir in discover(&roots) {
        let b = Arc::new(Backup::open(dir));
        println!("==================================================");
        println!("== {} — {} iOS {} ({:.2} GB)", b.summary.name, b.summary.model, b.summary.ios, b.summary.size as f64 / 1_000_000_000.0);
        if let Some(ddna) = b.ddna() {
            let active = ddna.flags.active_datasets();
            println!("  [DDNABackup] Datasets attivi ({}): {}", active.len(), active.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", "));
            if ddna.photos_disk_usage > 0 {
                println!("  [DDNABackup] Quote: Foto={:.2} GB, Audio={:.2} MB, App={:.2} GB",
                    ddna.photos_disk_usage as f64 / 1e9,
                    ddna.audio_disk_usage as f64 / 1e6,
                    ddna.apps_disk_usage as f64 / 1e9);
            }
        }
        let t = Instant::now();
        b.with(|cx| {
            println!("  app {}  domini {}  contatti {}", data::apps(cx)?.len(), data::domains(cx)?.len(), data::contacts(cx)?.len());
            println!("  chat sms {}  note {}", data::chats(cx)?.len(), data::notes(cx)?.len());

            if let Ok(ev) = calendars::events(cx) {
                println!("  calendari eventi: {} (primi: {})", ev.len(), ev.first().map(|e| e.summary.as_str()).unwrap_or("-"));
            }

            if let Ok(vm) = voicemail::voicemails(cx) {
                println!("  segreteria messaggi: {} (con audio: {})", vm.len(), vm.iter().filter(|v| v.audio_file_id.is_some()).count());
            }

            if let Ok(rec) = recordings::voice_memos(cx) {
                println!("  memo vocali: {} (con audio: {})", rec.len(), rec.iter().filter(|r| r.audio_file_id.is_some()).count());
            }

            if let Ok(bm) = safari_bookmarks::bookmarks(cx) {
                println!("  preferiti safari: {} (cartelle: {}, link: {})", bm.len(), bm.iter().filter(|b| b.is_folder).count(), bm.iter().filter(|b| !b.is_folder).count());
            }

            if let Ok(rems) = imancraft_core::reminders::reminders(cx) {
                println!("  promemoria: {} (completati: {})", rems.len(), rems.iter().filter(|r| r.completed).count());
            }

            if let Ok(findings) = spyware::scan_spyware(cx) {
                println!("  scansione spyware IOC: {} rilevamenti", findings.len());
            }

            let (p, a) = data::photos(cx)?;
            println!("  foto {} (iCloud {})  album {}", p.len(), p.iter().filter(|x| x.cloud).count(), a.len());

            if let Ok(c) = whatsapp::chats(cx) {
                if c.len() > 1 {
                    let page = whatsapp::messages(cx, c[1].id, &whatsapp::Filter::default(), whatsapp::Cursor::Latest, 50)?;
                    println!("  WA chat {}  pagina {} msg (older {})", c.len(), page.messages.len(), page.older);
                }
            }
            Ok(())
        })
        .unwrap_or_else(|e| println!("errore: {e}"));
        println!("Tempo analisi: ({:.2}s)", t.elapsed().as_secs_f32());
    }
}

