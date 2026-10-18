//! Ricerca globale unificata in tutto il backup:
//! Scansiona contemporaneamente Contatti, Note, Calendario, Promemoria, Chat WhatsApp e Messaggi.

use crate::calendars;
use crate::data;
use crate::reminders;
use crate::whatsapp;
use crate::{Ctx, Res};

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub category: &'static str,
    pub title: String,
    pub subtitle: String,
    pub date: Option<f64>,
}

pub fn global_search(cx: &Ctx, query: &str) -> Res<Vec<SearchResult>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let mut results = Vec::new();

    // 1. Contatti
    if let Ok(contacts) = data::contacts(cx) {
        for c in contacts {
            let full = format!("{} {} {:?} {:?}", c.name, c.org, c.phones, c.emails).to_lowercase();
            if full.contains(&q) {
                let phones = c.phones.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>().join(", ");
                results.push(SearchResult {
                    category: "Contatti",
                    title: c.name,
                    subtitle: if !c.org.is_empty() { format!("{} · {}", c.org, phones) } else { phones },
                    date: None,
                });
            }
        }
    }

    // 2. Note
    if let Ok(notes) = data::notes(cx) {
        for n in notes {
            if n.title.to_lowercase().contains(&q) || n.text.to_lowercase().contains(&q) {
                let sub = if n.text.len() > 120 {
                    format!("{}…", &n.text[..n.text.floor_char_boundary(120)].replace('\n', " "))
                } else {
                    n.text.replace('\n', " ")
                };
                results.push(SearchResult {
                    category: "Note",
                    title: n.title,
                    subtitle: sub,
                    date: Some(n.modified),
                });
            }
        }
    }

    // 3. Calendario
    if let Ok(events) = calendars::events(cx) {
        for e in events {
            if e.summary.to_lowercase().contains(&q) || e.location.to_lowercase().contains(&q) || e.description.to_lowercase().contains(&q) {
                results.push(SearchResult {
                    category: "Calendario",
                    title: e.summary,
                    subtitle: if !e.location.is_empty() { format!("{} · {}", e.calendar_title, e.location) } else { e.calendar_title },
                    date: Some(e.start_date),
                });
            }
        }
    }

    // 4. Promemoria
    if let Ok(rems) = reminders::reminders(cx) {
        for r in rems {
            if r.title.to_lowercase().contains(&q) || r.notes.to_lowercase().contains(&q) {
                results.push(SearchResult {
                    category: "Promemoria",
                    title: r.title,
                    subtitle: format!("Elenco: {} · {}", r.list, if r.completed { "Completato" } else { "Da fare" }),
                    date: r.due_date,
                });
            }
        }
    }

    // 5. WhatsApp
    if let Ok(chats) = whatsapp::chats(cx) {
        for chat in chats {
            if chat.title.to_lowercase().contains(&q) || chat.last_text.to_lowercase().contains(&q) {
                results.push(SearchResult {
                    category: "WhatsApp",
                    title: chat.title,
                    subtitle: if !chat.last_text.is_empty() { format!("{} · {}", chat.last_text, chat.count) } else { format!("{} messaggi", chat.count) },
                    date: Some(chat.last),
                });
            }
        }
    }

    // 6. Messaggi SMS / iMessage
    if let Ok(chats) = data::chats(cx) {
        for chat in chats {
            if chat.title.to_lowercase().contains(&q) {
                results.push(SearchResult {
                    category: "Messaggi",
                    title: chat.title,
                    subtitle: format!("Conversazione {} · {} messaggi", chat.service, chat.count),
                    date: Some(chat.last),
                });
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_struct() {
        let res = SearchResult {
            category: "Contatti",
            title: "Mario Rossi".into(),
            subtitle: "3331234567".into(),
            date: None,
        };
        assert_eq!(res.category, "Contatti");
    }
}
