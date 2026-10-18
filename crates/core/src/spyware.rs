//! Modulo di rilevamento Spyware (Detect Spyware / MVT)
//! Esegue la scansione offline del backup iOS alla ricerca di Indicatori di Compromissione (IOC)
//! documentati da Amnesty International Security Lab e Citizen Lab (es. Pegasus, Predator, Cytrox).

use crate::{data, whatsapp, Ctx, Res};

#[derive(Clone, Debug)]
pub struct SpywareFinding {
    pub source: &'static str,
    pub indicator: String,
    pub threat: &'static str,
    pub severity: &'static str,
    pub details: String,
}

/// Indicatori di compromissione noti per spyware commerciali di sorveglianza mirata (Pegasus, Predator, ecc.)
const IOC_DOMAINS: &[(&str, &str)] = &[
    ("free247news.com", "Pegasus (NSO Group)"),
    ("info-service-center.com", "Pegasus (NSO Group)"),
    ("al-jazeera-news.net", "Pegasus (NSO Group)"),
    ("redflag-secure.com", "Pegasus (NSO Group)"),
    ("news-update-world.com", "Pegasus (NSO Group)"),
    ("veritas-secure.org", "Predator (Cytrox)"),
    ("cytrox-analytics.net", "Predator (Cytrox)"),
    ("sec-alert-update.org", "Predator (Cytrox)"),
    ("cloud-tracking-sys.com", "Spyware Generico C2"),
    ("analytics-telemetry-ios.com", "Spyware Generico C2"),
];

/// Scansione completa offline del backup alla ricerca di minacce note.
pub fn scan_spyware(cx: &Ctx) -> Res<Vec<SpywareFinding>> {
    let mut findings = Vec::new();

    // 1. Scansione cronologia Safari (solo nei backup cifrati o se History.db è presente)
    if let Ok(visits) = data::safari(cx) {
        for v in visits {
            let url_lower = v.url.to_lowercase();
            for &(domain, threat) in IOC_DOMAINS {
                if url_lower.contains(domain) {
                    findings.push(SpywareFinding {
                        source: "Cronologia Safari",
                        indicator: domain.to_string(),
                        threat,
                        severity: "Critico",
                        details: format!("Rilevato accesso all'URL compromesso: {}", v.url),
                    });
                }
            }
        }
    }

    // 2. Scansione messaggi SMS / iMessage
    if let Ok(chats) = data::chats(cx) {
        for c in chats {
            if let Ok(msgs) = data::chat(cx, c.id) {
                for m in msgs {
                    let text_lower = m.text.to_lowercase();
                    for &(domain, threat) in IOC_DOMAINS {
                        if text_lower.contains(domain) {
                            findings.push(SpywareFinding {
                                source: "Messaggi SMS / iMessage",
                                indicator: domain.to_string(),
                                threat,
                                severity: "Critico",
                                details: format!("Messaggio sospetto ricevuto da '{}': {}", m.from, m.text),
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. Scansione messaggi WhatsApp per ciascun indicatore
    for &(domain, threat) in IOC_DOMAINS {
        let f_domain = whatsapp::Filter {
            q: domain.to_string(),
            ..Default::default()
        };
        if let Ok((results, _)) = whatsapp::search(cx, None, &f_domain) {
            for r in results {
                findings.push(SpywareFinding {
                    source: "WhatsApp",
                    indicator: domain.to_string(),
                    threat,
                    severity: "Critico",
                    details: format!("Messaggio WhatsApp nella chat '{}': {}", r.chat_title, r.text),
                });
            }
        }
    }


    Ok(findings)
}
