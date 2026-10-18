//! Lavori in background: ogni caricamento gira su un thread e compare tra le "Operazioni" della barra di stato.

use imancraft_core::{Backup, Ctx, Res, LOCKED};
use poll_promise::Promise;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type Task<T> = Promise<Result<T, String>>;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    #[allow(dead_code)]
    pub id: u64,
    pub label: String,
    pub duration_ms: u128,
    pub success: bool,
    pub error: Option<String>,
}

static OPS: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());
static HISTORY: Mutex<Vec<HistoryEntry>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Operazioni in corso (per la barra di stato).
pub fn running() -> Vec<String> {
    OPS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|o| o.1.clone()).collect()
}

/// Cronologia delle operazioni completate.
pub fn history() -> Vec<HistoryEntry> {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Svuota la cronologia delle operazioni.
pub fn clear_history() {
    HISTORY.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Avvia `f` su un thread; un panic diventa un errore invece di chiudere l'app.
pub fn run<T: Send + 'static>(ctx: &egui::Context, label: impl Into<String>, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Task<T> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let l_str = label.into();
    OPS.lock().unwrap_or_else(|e| e.into_inner()).push((id, l_str.clone()));
    let ctx = ctx.clone();
    let l_copy = l_str;
    let start = std::time::Instant::now();
    Promise::spawn_thread("imancraft-task", move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err("errore interno durante l'operazione".into()));
        let dur = start.elapsed().as_millis();
        let (success, err_msg) = match &r {
            Ok(_) => (true, None),
            Err(e) => (false, Some(e.clone())),
        };
        HISTORY.lock().unwrap_or_else(|e| e.into_inner()).push(HistoryEntry {
            id,
            label: l_copy,
            duration_ms: dur,
            success,
            error: err_msg,
        });
        OPS.lock().unwrap_or_else(|e| e.into_inner()).retain(|o| o.0 != id);
        ctx.request_repaint();
        r
    })
}

/// Lavoro sul backup aperto.
pub fn load<T: Send + 'static>(ctx: &egui::Context, label: impl Into<String>, b: &Arc<Backup>, f: impl FnOnce(&Ctx) -> Res<T> + Send + 'static) -> Task<T> {
    let b = b.clone();
    run(ctx, label, move || b.with(f).map_err(|e| e.to_string()))
}

/// Mostra lo stato di un caricamento: spinner, errore, oppure restituisce il risultato.
pub fn show<'a, T: Send>(ui: &mut egui::Ui, task: Option<&'a Task<T>>) -> Option<&'a T> {
    match task?.ready() {
        None => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Caricamento…");
            });
            None
        }
        Some(Err(e)) if e == LOCKED => {
            ui.weak("🔒 Backup cifrato: sbloccalo con la password nella sezione Dispositivi.");
            None
        }
        Some(Err(e)) => {
            ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {e}"));
            None
        }
        Some(Ok(v)) => Some(v),
    }
}
