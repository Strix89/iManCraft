//! WhatsApp: chat, conversazione, ricerca per intervallo data/ora, galleria, chiamate, export.

use super::{badge, bubble, export_file, fmt_date, fmt_size, header, paint_thumb, paint_thumb_opt, save_dialog, short, tz_offset_min, viewer, Env};
use crate::images::{self, Img, Src};
use crate::player::{fmt_time, Player};
use crate::tasks::{self, Task};
use crate::theme;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use jiff::civil::Date;
use egui::{Align, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Vec2};
use imancraft_core::media;
use imancraft_core::whatsapp::{self as wa, ChatKind, Cursor, ExportFormat, Filter, Kind, KindFilter, WaCall, WaChat, WaMessage, WaPage};

const PAGE: i64 = 300;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ChatFilter {
    #[default]
    All,
    Single,
    Group,
    Archived,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Search,
    Calls,
    Chat(i64),
}

/// Controlli del filtro: intervallo al minuto, testo, tipo, importanti.
#[derive(Clone)]
struct FilterUi {
    from_on: bool,
    from: Date,
    from_t: (u32, u32),
    to_on: bool,
    to: Date,
    to_t: (u32, u32),
    q: String,
    kind: KindFilter,
    starred: bool,
}

enum Action {
    None,
    Apply,
}

impl FilterUi {
    fn new(end: NaiveDateTime) -> FilterUi {
        FilterUi {
            from_on: false,
            from: jd(end.date() - chrono::Days::new(7)),
            from_t: (0, 0),
            to_on: false,
            to: jd(end.date()),
            to_t: (23, 59),
            q: String::new(),
            kind: KindFilter::All,
            starred: false,
        }
    }

    fn to_ts(d: Date, (h, m): (u32, u32)) -> Option<f64> {
        let dt = nd(d)?.and_time(NaiveTime::from_hms_opt(h, m, 0)?);
        Some(Local.from_local_datetime(&dt).earliest()?.timestamp() as f64)
    }

    fn filter(&self) -> Filter {
        Filter {
            from: if self.from_on { Self::to_ts(self.from, self.from_t) } else { None },
            // minuto finale incluso
            to: if self.to_on { Self::to_ts(self.to, self.to_t).map(|t| t + 60.0) } else { None },
            q: self.q.trim().to_string(),
            kind: self.kind,
            starred: self.starred,
        }
    }

    fn set_range(&mut self, from: NaiveDateTime, to: NaiveDateTime) {
        (self.from_on, self.from, self.from_t) = (true, jd(from.date()), from.time().hour_min());
        (self.to_on, self.to, self.to_t) = (true, jd(to.date()), to.time().hour_min());
    }

    /// Barra dei filtri. `end` = data del backup (i periodi rapidi partono da lì).
    fn ui(&mut self, ui: &mut egui::Ui, p: &theme::Palette, id: &str, end: NaiveDateTime, chat_pick: Option<(&mut Option<i64>, &[WaChat])>, apply_label: &str) -> Action {
        let mut act = Action::None;
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.from_on, "Da");
            ui.add_enabled_ui(self.from_on, |ui| {
                ui.add(egui_extras::DatePickerButton::new(&mut self.from).id_salt(&format!("{id}-from")).calendar_week(false));
                time_edit(ui, &mut self.from_t);
            });
            ui.add_space(6.0);
            ui.checkbox(&mut self.to_on, "A");
            ui.add_enabled_ui(self.to_on, |ui| {
                ui.add(egui_extras::DatePickerButton::new(&mut self.to).id_salt(&format!("{id}-to")).calendar_week(false));
                time_edit(ui, &mut self.to_t);
            });
            egui::ComboBox::from_id_salt(format!("{id}-preset")).selected_text("Periodo rapido…").show_ui(ui, |ui| {
                for (label, days) in [("Ultimo giorno", 1), ("Ultimi 7 giorni", 7), ("Ultimi 30 giorni", 30), ("Ultimi 3 mesi", 90), ("Ultimo anno", 365)] {
                    if ui.button(label).on_hover_text("Calcolato dalla data del backup").clicked() {
                        self.set_range(end - chrono::Duration::days(days), end);
                        act = Action::Apply;
                        ui.close();
                    }
                }
                ui.separator();
                for y in (2009..=end.year()).rev() {
                    if ui.button(format!("Anno {y}")).clicked() {
                        let a = NaiveDate::from_ymd_opt(y, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
                        let b = NaiveDate::from_ymd_opt(y, 12, 31).unwrap().and_hms_opt(23, 59, 0).unwrap();
                        self.set_range(a, b);
                        act = Action::Apply;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Tutto").clicked() {
                    self.from_on = false;
                    self.to_on = false;
                    act = Action::Apply;
                    ui.close();
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.q).hint_text("Testo da cercare…").desired_width(220.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                act = Action::Apply;
            }
            egui::ComboBox::from_id_salt(format!("{id}-kind"))
                .selected_text(KindFilter::ALL.iter().find(|k| k.0 == self.kind).map_or("", |k| k.1))
                .show_ui(ui, |ui| {
                    for (k, l) in KindFilter::ALL {
                        ui.selectable_value(&mut self.kind, k, l);
                    }
                });
            if let Some((sel, chats)) = chat_pick {
                let text = sel.and_then(|id| chats.iter().find(|c| c.id == id)).map_or("Tutte le chat".to_string(), |c| short(&c.title, 28));
                egui::ComboBox::from_id_salt(format!("{id}-chat")).selected_text(text).width(220.0).show_ui(ui, |ui| {
                    ui.selectable_value(sel, None, "Tutte le chat");
                    for c in chats {
                        ui.selectable_value(sel, Some(c.id), short(&c.title, 40));
                    }
                });
            }
            ui.checkbox(&mut self.starred, "⭐ Importanti");
            if theme::primary(ui, p, apply_label).clicked() {
                act = Action::Apply;
            }
            if ui.button("Azzera").clicked() {
                let end_keep = (self.to, self.from);
                *self = FilterUi { from: end_keep.1, to: end_keep.0, ..FilterUi::new(end) };
                act = Action::Apply;
            }
        });
        act
    }
}

/// Conversioni tra le date di jiff (calendario di egui_extras) e chrono (fusi orari locali di Windows).
fn jd(d: NaiveDate) -> Date {
    jiff::civil::date(d.year() as i16, d.month() as i8, d.day() as i8)
}

fn nd(d: Date) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32)
}

trait HourMin {
    fn hour_min(&self) -> (u32, u32);
}

impl HourMin for NaiveTime {
    fn hour_min(&self) -> (u32, u32) {
        use chrono::Timelike;
        (self.hour(), self.minute())
    }
}

fn time_edit(ui: &mut egui::Ui, t: &mut (u32, u32)) {
    ui.spacing_mut().item_spacing.x = 2.0;
    ui.add(egui::DragValue::new(&mut t.0).range(0..=23).custom_formatter(|v, _| format!("{v:02}")));
    ui.label(":");
    ui.add(egui::DragValue::new(&mut t.1).range(0..=59).custom_formatter(|v, _| format!("{v:02}")));
}

pub struct State {
    chats: Option<Task<Vec<WaChat>>>,
    filter: String,
    kind: ChatFilter,
    mode: Mode,
    // conversazione
    pending: Option<(Cursor, Task<WaPage>)>,
    msgs: Vec<WaMessage>,
    older: bool,
    newer: bool,
    scroll_to: Option<(i64, Align)>,
    flash: Option<i64>,
    to_bottom: bool,
    chat_filter: Option<FilterUi>,
    applied: Filter,
    gallery: Option<Task<Vec<WaMessage>>>,
    // ricerca globale
    search: Option<FilterUi>,
    search_chat: Option<i64>,
    results: Option<(Filter, Task<(Vec<WaMessage>, bool)>)>,
    calls: Option<Task<Vec<WaCall>>>,
    export: Option<Task<()>>,
    audio: Option<(i64, Player)>,
}

impl Default for State {
    fn default() -> Self {
        State {
            chats: None,
            filter: String::new(),
            kind: ChatFilter::All,
            mode: Mode::Search,
            pending: None,
            msgs: vec![],
            older: false,
            newer: false,
            scroll_to: None,
            flash: None,
            to_bottom: false,
            chat_filter: None,
            applied: Filter::default(),
            gallery: None,
            search: None,
            search_chat: None,
            results: None,
            calls: None,
            export: None,
            audio: None,
        }
    }
}

/// Data del backup (fine dei periodi rapidi), ora locale.
fn backup_end(env: &Env) -> NaiveDateTime {
    chrono::DateTime::parse_from_rfc3339(&env.backup.summary.date).map(|d| d.with_timezone(&Local).naive_local()).unwrap_or_else(|_| Local::now().naive_local())
}

impl State {
    fn open_chat(&mut self, ctx: &egui::Context, env: &Env, id: i64, cursor: Cursor) {
        self.mode = Mode::Chat(id);
        self.gallery = None;
        self.audio = None;
        self.msgs.clear();
        self.applied = Filter::default();
        self.chat_filter = Some(FilterUi::new(backup_end(env)));
        self.load(ctx, env, cursor);
    }

    fn load(&mut self, ctx: &egui::Context, env: &Env, cursor: Cursor) {
        let Mode::Chat(chat) = self.mode else { return };
        let f = self.applied.clone();
        self.pending = Some((cursor, tasks::load(ctx, "Messaggi WhatsApp", env.backup, move |cx| wa::messages(cx, chat, &f, cursor, PAGE))));
    }

    /// Integra la pagina arrivata.
    fn receive(&mut self) {
        let Some((cursor, task)) = &self.pending else { return };
        let Some(res) = task.ready() else { return };
        if let Ok(page) = res {
            match *cursor {
                Cursor::Before(_) => {
                    // resto fermo sul primo messaggio visto finora
                    self.scroll_to = self.msgs.first().map(|m| (m.pk, Align::TOP));
                    let mut v = page.messages.clone();
                    v.append(&mut self.msgs);
                    self.msgs = v;
                    self.older = page.older;
                }
                Cursor::After(_) => {
                    self.msgs.extend(page.messages.iter().cloned());
                    self.newer = page.newer;
                }
                Cursor::Around(pk) => {
                    (self.msgs, self.older, self.newer) = (page.messages.clone(), page.older, page.newer);
                    self.scroll_to = Some((pk, Align::Center));
                    self.flash = Some(pk);
                }
                Cursor::Latest => {
                    (self.msgs, self.older, self.newer) = (page.messages.clone(), page.older, page.newer);
                    if self.applied.from.is_some() {
                        self.scroll_to = self.msgs.first().map(|m| (m.pk, Align::TOP));
                    } else {
                        self.to_bottom = true;
                    }
                }
            }
            self.pending = None;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        if self.chats.is_none() {
            self.chats = Some(tasks::load(ui.ctx(), "Chat WhatsApp", env.backup, wa::chats));
        }
        self.receive();
        let chats: Vec<WaChat> = match self.chats.as_ref().and_then(|t| t.ready()) {
            Some(Ok(c)) => c.clone(),
            _ => {
                header(ui, env.p, "05", "WhatsApp", "WhatsApp");
                tasks::show(ui, self.chats.as_ref());
                return;
            }
        };
        self.chat_list(ui, env, &chats);
        match self.mode {
            Mode::Search => self.search_view(ui, env, &chats),
            Mode::Calls => self.calls_view(ui, env),
            Mode::Chat(id) => {
                let chat = chats.iter().find(|c| c.id == id).cloned();
                self.chat_view(ui, env, chat);
            }
        }
    }

    fn chat_list(&mut self, ui: &mut egui::Ui, env: &mut Env, chats: &[WaChat]) {
        super::side_panel("wa-chats", env.p).show(ui, |ui| {
            header(ui, env.p, "05", "WhatsApp", "WhatsApp");
            ui.horizontal(|ui| {
                if ui.selectable_label(self.mode == Mode::Search, "🔎 Ricerca per periodo").clicked() {
                    self.mode = Mode::Search;
                }
                if ui.selectable_label(self.mode == Mode::Calls, "📞 Chiamate").clicked() {
                    self.mode = Mode::Calls;
                }
            });
            super::filter_box(ui, &mut self.filter, &format!("Cerca tra {} chat…", chats.len()));
            ui.horizontal_wrapped(|ui| {
                for (k, l) in [(ChatFilter::All, "Tutte"), (ChatFilter::Single, "Singole"), (ChatFilter::Group, "Gruppi"), (ChatFilter::Archived, "Archiviate"), (ChatFilter::Other, "Altro")] {
                    ui.selectable_value(&mut self.kind, k, l);
                }
            });
            let f = self.filter.to_lowercase();
            let list: Vec<&WaChat> = chats
                .iter()
                .filter(|c| f.is_empty() || format!("{} {}", c.title, c.jid).to_lowercase().contains(&f))
                .filter(|c| match self.kind {
                    ChatFilter::All => true,
                    ChatFilter::Single => c.kind == ChatKind::Chat,
                    ChatFilter::Group => c.kind == ChatKind::Group,
                    ChatFilter::Archived => c.archived,
                    ChatFilter::Other => !matches!(c.kind, ChatKind::Chat | ChatKind::Group),
                })
                .collect();
            let mut open = None;
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 52.0, list.len(), |ui, range| {
                for c in &list[range] {
                    let sel = self.mode == Mode::Chat(c.id);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), Sense::click());
                    if sel {
                        ui.painter().rect_filled(rect, 4.0, env.p.accent.gamma_multiply(0.18));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 4.0, env.p.panel_alt);
                    }
                    avatar(ui, env, c, egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 6.0), Vec2::splat(38.0)));
                    let x = rect.min.x + 50.0;
                    let w = rect.width() - 54.0;
                    let pt = ui.painter_at(rect);
                    let title = format!("{}{}{}", if c.kind == ChatKind::Group { "👥 " } else { "" }, if c.archived { "🗄 " } else { "" }, c.title);
                    pt.text(egui::pos2(x, rect.min.y + 6.0), egui::Align2::LEFT_TOP, short(&title, ((w - 80.0) / 8.0) as usize), egui::FontId::proportional(13.5), env.p.text);
                    pt.text(egui::pos2(x, rect.min.y + 24.0), egui::Align2::LEFT_TOP, short(&c.last_text.replace('\n', " "), (w / 6.5) as usize), egui::FontId::proportional(11.5), env.p.muted);
                    pt.text(egui::pos2(rect.max.x - 4.0, rect.min.y + 6.0), egui::Align2::RIGHT_TOP, fmt_date(c.last).get(..10).unwrap_or(""), egui::FontId::proportional(10.5), env.p.muted);
                    if resp.clicked() {
                        open = Some(c.id);
                    }
                }
            });
            if let Some(id) = open {
                self.open_chat(ui.ctx(), env, id, Cursor::Latest);
            }
        });
    }

    fn search_view(&mut self, ui: &mut egui::Ui, env: &mut Env, chats: &[WaChat]) {
        let end = backup_end(env);
        theme::section_label(ui, env.p, "Ricerca nei messaggi");
        theme::title(ui, "🔎 Ricerca per periodo");
        ui.weak("Filtra per intervallo di date e orari (al minuto), testo, tipo di contenuto, chat e messaggi importanti. I periodi rapidi partono dalla data del backup.");
        let fu = self.search.get_or_insert_with(|| FilterUi::new(end));
        let act = fu.ui(ui, env.p, "wa-search", end, Some((&mut self.search_chat, chats)), "Cerca");
        if let Action::Apply = act {
            let f = fu.filter();
            if f.is_empty() {
                self.results = None;
            } else {
                let (f2, chat) = (f.clone(), self.search_chat);
                self.results = Some((f, tasks::load(ui.ctx(), "Ricerca WhatsApp", env.backup, move |cx| wa::search(cx, chat, &f2))));
            }
        }
        ui.separator();
        let Some((f, task)) = &self.results else {
            ui.weak("Imposta almeno un filtro (periodo, testo, tipo o importanti) e premi Cerca. Clic su un risultato per aprirlo nella conversazione.");
            return;
        };
        let q = f.q.to_lowercase();
        let Some((hits, truncated)) = tasks::show(ui, Some(task)) else { return };
        ui.label(format!("{} risultati{}", hits.len(), if *truncated { " (mostrati i più recenti 1000: restringi il periodo)" } else { "" }));
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, 46.0, hits.len(), |ui, range| {
            for h in &hits[range] {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, 3.0, env.p.panel_alt);
                }
                let pt = ui.painter_at(rect);
                pt.text(rect.min + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, format!("{} · {}", short(&h.chat_title, 50), fmt_date(h.t)), egui::FontId::proportional(11.0), env.p.muted);
                let body = if h.text.is_empty() { h.label.to_string() } else { h.text.replace('\n', " ") };
                // porzione di testo intorno alla parola cercata
                let snippet = match (!q.is_empty()).then(|| body.to_lowercase().find(&q)).flatten() {
                    Some(i) if i > 40 => format!("…{}", body.get(body.floor_char_boundary(i - 30)..).unwrap_or(&body)),
                    _ => body,
                };
                let mut job = egui::text::LayoutJob::default();
                job.append(&format!("{}: ", h.from), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: env.p.accent, ..Default::default() });
                job.append(&short(&snippet, ((rect.width() - 120.0) / 6.8) as usize), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: env.p.text, ..Default::default() });
                pt.galley(rect.min + Vec2::new(6.0, 21.0), ui.fonts_mut(|f| f.layout_job(job)), env.p.text);
                if resp.clicked() {
                    open = Some((h.chat, h.pk));
                }
            }
        });
        if let Some((chat, pk)) = open {
            self.open_chat(ui.ctx(), env, chat, Cursor::Around(pk));
        }
    }

    fn calls_view(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        theme::section_label(ui, env.p, "Chiamate WhatsApp");
        theme::title(ui, "📞 Chiamate");
        if self.calls.is_none() {
            self.calls = Some(tasks::load(ui.ctx(), "Chiamate WhatsApp", env.backup, wa::calls));
        }
        let Some(list) = tasks::show(ui, self.calls.as_ref()) else { return };
        ui.weak(format!("{} chiamate", list.len()));
        let p = *env.p;
        egui_extras::TableBuilder::new(ui)
            .striped(true)
            .column(egui_extras::Column::exact(28.0))
            .column(egui_extras::Column::initial(320.0))
            .column(egui_extras::Column::initial(170.0))
            .column(egui_extras::Column::initial(150.0))
            .column(egui_extras::Column::remainder())
            .header(22.0, |h| super::lists::head(&["", "CONTATTO", "TIPO", "DATA", "DURATA"], h))
            .body(|body| {
                body.rows(24.0, list.len(), |mut row| {
                    let c = &list[row.index()];
                    row.col(|ui| {
                        if c.missed {
                            ui.colored_label(p.danger, "✗");
                        } else {
                            ui.label(if c.incoming { "↙" } else { "↗" });
                        }
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(c.group.as_deref().unwrap_or(if c.who.is_empty() { "—" } else { &c.who })).strong()).on_hover_text(&c.who);
                    });
                    row.col(|ui| {
                        ui.label(format!("{}{}{}", if c.video { "📹 Video" } else { "📞 Voce" }, if c.group.is_some() { " di gruppo" } else { "" }, if c.missed { " · persa" } else { "" }));
                    });
                    row.col(|ui| {
                        ui.weak(fmt_date(c.t));
                    });
                    row.col(|ui| {
                        ui.weak(fmt_time(c.duration));
                    });
                });
            });
    }

    fn chat_view(&mut self, ui: &mut egui::Ui, env: &mut Env, chat: Option<WaChat>) {
        let Some(chat) = chat else { return };
        let end = backup_end(env);
        let p = *env.p;
        // intestazione
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), Sense::hover());
            avatar(ui, env, &chat, rect);
            ui.vertical(|ui| {
                theme::title(ui, &short(&chat.title, 60));
                ui.weak(format!("{} messaggi · {}", chat.count, chat.jid));
            });
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let busy = self.export.as_ref().is_some_and(|t| t.ready().is_none());
                if ui.add_enabled(!busy, egui::Button::new("⬇ HTML (Stampa/PDF)")).on_hover_text("Esporta la chat in formato HTML pronto da aprire e stampare in PDF").clicked() {
                    if let Some(dest) = save_dialog(&format!("WhatsApp - {}.html", chat.title)) {
                        let (id, f, tz, title) = (chat.id, self.applied.clone(), tz_offset_min(), chat.title.clone());
                        self.export = Some(tasks::load(ui.ctx(), "Esportazione chat HTML", env.backup, move |cx| {
                            imancraft_core::export::export_whatsapp_html(&title, cx, id, &f, tz, &dest)
                        }));
                    }
                }
                if ui.add_enabled(!busy, egui::Button::new("⬇ XLSX (Excel)")).on_hover_text("Esporta la chat in formato foglio di calcolo Excel").clicked() {
                    if let Some(dest) = save_dialog(&format!("WhatsApp - {}.xlsx", chat.title)) {
                        let (id, f, tz) = (chat.id, self.applied.clone(), tz_offset_min());
                        self.export = Some(tasks::load(ui.ctx(), "Esportazione chat XLSX", env.backup, move |cx| {
                            imancraft_core::export::export_whatsapp_xlsx(cx, id, &f, tz, &dest)
                        }));
                    }
                }
                for (label, fmt) in [("⬇ CSV (Excel)", ExportFormat::Csv), ("⬇ TXT", ExportFormat::Txt)] {
                    if ui.add_enabled(!busy, egui::Button::new(label)).on_hover_text("Esporta la chat con i filtri attivi").clicked() {
                        let ext = if fmt == ExportFormat::Csv { "csv" } else { "txt" };
                        if let Some(dest) = save_dialog(&format!("WhatsApp - {}.{ext}", chat.title)) {
                            let (id, f, tz) = (chat.id, self.applied.clone(), tz_offset_min());
                            self.export = Some(tasks::load(ui.ctx(), "Esportazione chat", env.backup, move |cx| {
                                let (_, text) = wa::export(cx, id, &f, fmt, tz)?;
                                std::fs::write(&dest, text)?;
                                Ok(())
                            }));
                        }
                    }
                }

                if ui.selectable_label(self.gallery.is_some(), "🖼 Media, documenti e link").clicked() {
                    self.gallery = if self.gallery.is_some() {
                        None
                    } else {
                        let id = chat.id;
                        Some(tasks::load(ui.ctx(), "Media della chat", env.backup, move |cx| wa::media(cx, id)))
                    };
                }
                if busy {
                    ui.spinner();
                }
                match self.export.as_ref().and_then(|t| t.ready()) {
                    Some(Err(e)) => {
                        ui.colored_label(p.danger, format!("⚠ {e}"));
                    }
                    Some(Ok(())) => {
                        ui.colored_label(p.success, "✓ esportata");
                    }
                    None => {}
                }
            });
        });
        let fu = self.chat_filter.get_or_insert_with(|| FilterUi::new(end));
        if let Action::Apply = fu.ui(ui, &p, "wa-chat", end, None, "Applica") {
            self.applied = fu.filter();
            self.gallery = None;
            self.load(ui.ctx(), env, Cursor::Latest);
        }
        ui.separator();
        if self.gallery.is_some() {
            self.gallery_view(ui, env);
            return;
        }
        if !self.applied.is_empty() {
            ui.weak(format!("Filtro attivo: {} messaggi caricati{}", self.msgs.len(), if self.older || self.newer { " (altri disponibili)" } else { "" }));
        }
        let loading = self.pending.is_some();
        // media della pagina, per il visualizzatore
        let media: Vec<viewer::Item> = self
            .msgs
            .iter()
            .filter(|m| matches!(m.kind, Kind::Image | Kind::Video))
            .filter_map(|m| {
                let md = m.media.as_ref()?;
                (md.id.is_some() || md.thumb.is_some()).then(|| viewer::Item {
                    backup: env.backup.clone(),
                    id: md.id.clone(),
                    thumb: md.thumb.clone(),
                    name: md.name.clone(),
                    t: m.t,
                    size: md.size as u64,
                    video: m.kind == Kind::Video,
                    cloud: false,
                    w: 0,
                    h: 0,
                })
            })
            .collect();
        let mut action: Option<Cursor> = None;
        let names = chat.kind != ChatKind::Chat;
        egui::ScrollArea::vertical().id_salt(("wa-msgs", chat.id)).auto_shrink(false).show(ui, |ui| {
            if self.older && ui.add_enabled(!loading, egui::Button::new("⬆ Carica messaggi precedenti")).clicked() {
                action = self.msgs.first().map(|m| Cursor::Before(m.sort));
            }
            if self.msgs.is_empty() {
                if loading {
                    ui.spinner();
                } else {
                    ui.weak("Nessun messaggio con questi filtri.");
                }
            }
            let mut day = String::new();
            let msgs = std::mem::take(&mut self.msgs);
            for m in &msgs {
                let d = fmt_date(m.t).get(..10).unwrap_or("").to_string();
                if d != day {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        let text = if d.is_empty() { "data sconosciuta" } else { &d };
                        let font_id = egui::FontId::proportional(11.0);
                        let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, p.muted);
                        let pad = egui::vec2(18.0, 6.0);
                        let size = galley.size() + pad;
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        ui.painter().rect_filled(rect, 8.0, p.panel_alt);
                        ui.painter().galley(rect.min + egui::vec2(9.0, 3.0), galley, p.muted);
                    });
                    ui.add_space(4.0);
                    day = d;
                }
                let r = self.message(ui, env, m, names, &media);
                if self.flash == Some(m.pk) {
                    ui.painter().rect_stroke(r.rect.expand(2.0), 10.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                }
                if self.scroll_to.is_some_and(|(pk, _)| pk == m.pk) {
                    r.scroll_to_me(self.scroll_to.map(|s| s.1));
                    self.scroll_to = None;
                }
            }
            self.msgs = msgs;
            if self.newer && ui.add_enabled(!loading, egui::Button::new("⬇ Carica messaggi successivi")).clicked() {
                action = self.msgs.last().map(|m| Cursor::After(m.sort));
            }
            if self.to_bottom {
                ui.scroll_to_cursor(Some(Align::BOTTOM));
                self.to_bottom = false;
            }
        });
        if let Some(c) = action {
            self.load(ui.ctx(), env, c);
        }
    }

    /// Un messaggio (bolla o evento centrato).
    fn message(&mut self, ui: &mut egui::Ui, env: &mut Env, m: &WaMessage, names: bool, media: &[viewer::Item]) -> egui::Response {
        let p = *env.p;
        if matches!(m.kind, Kind::Event | Kind::System) {
            return ui
                .vertical_centered(|ui| {
                    ui.label(RichText::new(format!("{} · {}", if m.text.is_empty() { m.label } else { &m.text }, fmt_date(m.t).get(11..).unwrap_or(""))).small().color(p.muted))
                })
                .inner;
        }
        ui.add_space(2.0);
        bubble(ui, &p, m.me, m.pk, |ui| {
            if names && !m.me && !m.from.is_empty() {
                ui.label(RichText::new(&m.from).small().strong().color(if m.me { p.on_accent } else { p.accent }));
            }
            if let Some(q) = &m.quote {
                Frame::new().fill(if m.me { p.on_accent.gamma_multiply(0.15) } else { p.panel_alt }).corner_radius(CornerRadius::same(5)).inner_margin(Margin::symmetric(6, 3)).show(ui, |ui| {
                    ui.label(RichText::new(&q.from).small().strong());
                    ui.add(egui::Label::new(RichText::new(short(&q.text, 160)).small()).wrap());
                });
            }
            let md = m.media.as_ref();
            let mut caption = true;
            match m.kind {
                Kind::Image | Kind::Video => {
                    let vid = m.kind == Kind::Video;
                    let has_media = md.is_some_and(|x| x.id.is_some() || x.thumb.is_some());
                    if has_media {
                        let md = md.unwrap();
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(220.0, 220.0), Sense::click());
                        match (&md.thumb, &md.id) {
                            (Some(th), Some(id)) => paint_thumb_opt(ui, env, rect, th, "t.jpg", Some(id), Some(&md.name)),
                            (Some(th), None) => paint_thumb(ui, env, rect, th, "t.jpg"),
                            (None, Some(id)) => paint_thumb(ui, env, rect, id, &md.name),
                            _ => {}
                        }
                        if vid {
                            badge(ui, rect, &format!("▶ {}", fmt_time(md.duration)), true);
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            let key = md.id.clone().or_else(|| md.thumb.clone());
                            let k = media.iter().position(|it| it.id.clone().or_else(|| it.thumb.clone()) == key).unwrap_or(0);
                            *env.viewer = Some(viewer::Viewer::new(media.to_vec(), k));
                        }
                    } else {
                        ui.weak(format!("{} — non presente nel backup", m.label));
                    }
                }
                Kind::Sticker => match md.and_then(|x| x.id.clone().map(|i| (i, x.name.clone()))) {
                    Some((id, name)) => {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(130.0), Sense::hover());
                        match env.images.get(&images::key(env.backup, &id, 256), 256, || Src::File { backup: env.backup.clone(), id: id.clone(), name }) {
                            Img::Ready(t) => {
                                ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                            }
                            _ => {
                                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "sticker", egui::FontId::proportional(11.0), p.muted);
                            }
                        }
                    }
                    None => {
                        ui.weak("Sticker non presente nel backup");
                    }
                },
                Kind::Audio => match md.and_then(|x| x.id.clone().map(|i| (i, x.duration))) {
                    Some((id, dur)) => {
                        if self.audio.as_ref().is_some_and(|a| a.0 == m.pk) {
                            ui.set_min_width(320.0);
                            self.audio.as_mut().unwrap().1.controls(ui);
                        } else if ui.button(format!("▶ {}  {}", if m.label.is_empty() { "Audio" } else { m.label }, fmt_time(dur))).clicked() {
                            let url = media::url(media::register(env.backup), &id);
                            self.audio = Some((m.pk, Player::open(url)));
                        }
                    }
                    None => {
                        ui.weak(format!("{} — non presente nel backup", m.label));
                    }
                },
                Kind::Doc => {
                    caption = false;
                    let title = if !m.title.is_empty() { m.title.clone() } else if !m.text.is_empty() { m.text.clone() } else { md.map_or("Documento".into(), |x| x.name.clone()) };
                    match md.and_then(|x| x.id.clone().map(|i| (i, x.name.clone(), x.size))) {
                        Some((id, name, size)) => {
                            ui.horizontal(|ui| {
                                ui.label(format!("📄 {}", short(&title, 50)));
                                ui.weak(fmt_size(size as u64));
                                if ui.small_button("Esporta…").clicked() {
                                    let ext = name.rsplit_once('.').map(|x| x.1).unwrap_or("bin");
                                    let fname = if title.contains('.') { title.clone() } else { format!("{title}.{ext}") };
                                    self.export = export_file(ui.ctx(), env.backup, &id, &fname).or(self.export.take());
                                }
                            });
                        }
                        None => {
                            ui.weak(format!("📄 {title} — non presente nel backup"));
                        }
                    }
                }
                Kind::Location => {
                    let url = format!("https://www.openstreetmap.org/?mlat={0}&mlon={1}#map=16/{0}/{1}", m.lat, m.lon);
                    ui.hyperlink_to(format!("📍 {:.5}, {:.5}", m.lat, m.lon), url);
                    if !m.title.is_empty() {
                        ui.add(egui::Label::new(&m.title).wrap());
                    }
                }
                Kind::Contact => {
                    ui.label(RichText::new(format!("👤 {}", m.vcard_name)).strong());
                    if !m.vcard.is_empty() {
                        egui::CollapsingHeader::new("vCard").id_salt(m.pk).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(&m.vcard).monospace().small()).selectable(true).wrap());
                        });
                    }
                }
                Kind::Link => {
                    if !m.title.is_empty() {
                        ui.add(egui::Label::new(RichText::new(format!("🔗 {}", m.title)).strong()).wrap());
                    }
                }
                Kind::Deleted | Kind::Poll | Kind::Other => {
                    ui.add(egui::Label::new(RichText::new(m.label).italics()).wrap());
                }
                Kind::Text | Kind::Event | Kind::System => {}
            }
            if caption && !m.text.is_empty() {
                ui.add(egui::Label::new(&m.text).selectable(true).wrap());
                // link cliccabili (solo http/https)
                for url in m.text.split_whitespace().filter(|w| w.starts_with("http://") || w.starts_with("https://")).take(3) {
                    ui.hyperlink_to(RichText::new(short(url, 60)).small(), url);
                }
            }
            ui.label(RichText::new(format!("{}{}", if m.starred { "⭐ " } else { "" }, fmt_date(m.t).get(11..).unwrap_or(""))).small().weak());
        })
    }

    fn gallery_view(&mut self, ui: &mut egui::Ui, env: &mut Env) {
        let Some(list) = tasks::show(ui, self.gallery.as_ref()) else { return };
        let vis: Vec<viewer::Item> = list
            .iter()
            .filter(|m| matches!(m.kind, Kind::Image | Kind::Video))
            .filter_map(|m| {
                let md = m.media.as_ref()?;
                (md.id.is_some() || md.thumb.is_some()).then(|| viewer::Item {
                    backup: env.backup.clone(),
                    id: md.id.clone(),
                    thumb: md.thumb.clone(),
                    name: md.name.clone(),
                    t: m.t,
                    size: md.size as u64,
                    video: m.kind == Kind::Video,
                    cloud: false,
                    w: 0,
                    h: 0,
                })
            })
            .collect();
        let docs: Vec<&WaMessage> = list.iter().filter(|m| m.kind == Kind::Doc).collect();
        let auds: Vec<&WaMessage> = list.iter().filter(|m| m.kind == Kind::Audio).collect();
        let links: Vec<&WaMessage> = list.iter().filter(|m| m.kind == Kind::Link).collect();
        let mut open = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            theme::section_label(ui, env.p, &format!("Foto e video ({})", vis.len()));
            ui.horizontal_wrapped(|ui| {
                for (k, it) in vis.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(110.0), Sense::click());
                    match (&it.thumb, &it.id) {
                        (Some(th), Some(id)) => paint_thumb_opt(ui, env, rect, th, "t.jpg", Some(id), Some(&it.name)),
                        (Some(th), None) => paint_thumb(ui, env, rect, th, "t.jpg"),
                        (None, Some(id)) => paint_thumb(ui, env, rect, id, &it.name),
                        _ => {}
                    }
                    if it.video {
                        badge(ui, rect, "▶", true);
                    }
                    if resp.clicked() {
                        open = Some(k);
                    }
                }
            });
            ui.add_space(10.0);
            theme::section_label(ui, env.p, &format!("Documenti ({})", docs.len()));
            for m in &docs {
                ui.horizontal(|ui| {
                    ui.weak(fmt_date(m.t));
                    ui.label(&m.from);
                    let title = if !m.title.is_empty() { &m.title } else { &m.text };
                    ui.label(format!("📄 {}", short(title, 60)));
                    if let Some((id, name)) = m.media.as_ref().and_then(|x| x.id.clone().map(|i| (i, x.name.clone()))) {
                        if ui.small_button("Esporta…").clicked() {
                            let fname = if title.contains('.') { title.clone() } else { name };
                            self.export = export_file(ui.ctx(), env.backup, &id, &fname).or(self.export.take());
                        }
                    } else {
                        ui.weak("(non nel backup)");
                    }
                });
            }
            ui.add_space(10.0);
            theme::section_label(ui, env.p, &format!("Audio e note vocali ({})", auds.len()));
            for m in &auds {
                ui.horizontal(|ui| {
                    ui.weak(fmt_date(m.t));
                    ui.label(&m.from);
                    match m.media.as_ref().and_then(|x| x.id.clone().map(|i| (i, x.duration))) {
                        Some((id, dur)) => {
                            if self.audio.as_ref().is_some_and(|a| a.0 == m.pk) {
                                ui.set_min_width(420.0);
                                self.audio.as_mut().unwrap().1.controls(ui);
                            } else if ui.button(format!("▶ {}", fmt_time(dur))).clicked() {
                                self.audio = Some((m.pk, Player::open(media::url(media::register(env.backup), &id))));
                            }
                        }
                        None => {
                            ui.weak("(non nel backup)");
                        }
                    }
                });
            }
            ui.add_space(10.0);
            theme::section_label(ui, env.p, &format!("Link ({})", links.len()));
            for m in &links {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(fmt_date(m.t));
                    ui.label(&m.from);
                    for url in m.text.split_whitespace().filter(|w| w.starts_with("http://") || w.starts_with("https://")).take(2) {
                        ui.hyperlink_to(short(url, 70), url);
                    }
                    if !m.title.is_empty() {
                        ui.weak(short(&m.title, 60));
                    }
                });
            }
        });
        if let Some(k) = open {
            *env.viewer = Some(viewer::Viewer::new(vis, k));
        }
    }
}

/// Foto profilo rotonda (o iniziali).
fn avatar(ui: &egui::Ui, env: &Env, c: &WaChat, rect: egui::Rect) {
    let p = env.p;
    if let Some(id) = &c.avatar {
        if let Img::Ready(t) = env.images.get(&images::key(env.backup, id, 96), 96, || Src::File { backup: env.backup.clone(), id: id.clone(), name: "a.thumb".into() }) {
            egui::Image::new(&t).corner_radius(CornerRadius::same((rect.width() / 2.0) as u8)).paint_at(ui, rect);
            return;
        }
    }
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, p.panel_alt);
    let ini: String = c.title.split_whitespace().filter_map(|w| w.chars().find(|ch| ch.is_alphanumeric())).take(2).collect::<String>().to_uppercase();
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, if ini.is_empty() { "#".into() } else { ini }, egui::FontId::proportional(14.0), p.muted);
}
