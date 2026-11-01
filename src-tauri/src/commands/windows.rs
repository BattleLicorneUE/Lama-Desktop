//! Fenêtres supplémentaires et tableau des tâches en cours.

use crate::*;

/// Ouvre une nouvelle fenêtre sur le même profil : chaque fenêtre a sa conversation, son dossier de travail et son agent.
#[tauri::command]
pub(crate) async fn window_new(app: AppHandle, state: State<'_, AppState>, task: Option<TaskSpec>) -> Result<String, String> {
    active_profile(&app)?;
    if let Some(t) = &task {
        if t.prompt.trim().is_empty() {
            return Err("Décris la tâche à lancer.".into());
        }
        if let Some(w) = t.workspace.as_deref().filter(|w| !w.trim().is_empty()) {
            if !std::path::Path::new(w).is_dir() {
                return Err(format!("Dossier de travail introuvable : {w}"));
            }
        }
    }
    let label = format!("win-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    if let Some(t) = task {
        state.tasks.lock_safe().insert(label.clone(), t);
    }
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title("Lama Desktop")
        .inner_size(1180.0, 780.0)
        .min_inner_size(900.0, 560.0)
        .decorations(false)
        .shadow(true)
        .build()
        .map_err(|e| format!("Fenêtre impossible : {e}"))?;
    Ok(label)
}

/// Tâche à démarrer dans cette fenêtre (lancée depuis le tableau des agents), une seule fois.
#[tauri::command]
pub(crate) fn window_task(window: tauri::Window, state: State<'_, AppState>) -> Option<TaskSpec> {
    state.tasks.lock_safe().remove(window.label())
}

/// Chaque fenêtre dit ce qu'elle fait ; le tableau des agents les affiche toutes.
#[tauri::command]
pub(crate) fn board_report(window: tauri::Window, state: State<'_, AppState>, entry: BoardEntry) {
    let mut e = entry;
    e.label = window.label().to_string();
    state.board.lock_safe().insert(e.label.clone(), e);
}

#[tauri::command]
pub(crate) fn board_list(app: AppHandle, state: State<'_, AppState>) -> Vec<BoardEntry> {
    // Les fenêtres d'impression PDF (cachées, éphémères) n'ont rien à faire dans le tableau.
    let open: HashMap<String, tauri::WebviewWindow> = app.webview_windows().into_iter().filter(|(l, _)| !l.starts_with("pdf-")).collect();
    let mut board = state.board.lock_safe();
    board.retain(|label, _| open.contains_key(label));
    let mut out: Vec<BoardEntry> = open
        .keys()
        .map(|label| {
            board.get(label).cloned().unwrap_or_else(|| BoardEntry { label: label.clone(), state: "idle".into(), ..Default::default() })
        })
        .collect();
    // La fenêtre principale d'abord, puis par ordre alphabétique des étiquettes.
    out.sort_by(|a, b| (a.label != "main", &a.label).cmp(&(b.label != "main", &b.label)));
    out
}

#[tauri::command]
pub(crate) fn board_focus(app: AppHandle, label: String) -> Result<(), String> {
    let w = app.get_webview_window(&label).ok_or("Fenêtre introuvable.")?;
    let _ = w.unminimize();
    let _ = w.show();
    w.set_focus().map_err(|e| e.to_string())
}
