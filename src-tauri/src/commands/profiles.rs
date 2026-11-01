//! Profils, code PIN et clé API.

use crate::*;

/// Clé API du profil ouvert, depuis le coffre Windows.
#[tauri::command]
pub(crate) fn secret_get(app: AppHandle) -> Result<Option<String>, String> {
    secrets::get(&active_profile(&app)?, "api-key")
}

/// Enregistre (ou, si vide, supprime) la clé API du profil ouvert dans le coffre Windows.
#[tauri::command]
pub(crate) fn secret_set(app: AppHandle, value: String) -> Result<(), String> {
    secrets::set(&active_profile(&app)?, "api-key", value.trim())
}

#[tauri::command]
pub(crate) fn profiles_list(app: AppHandle) -> Result<Vec<profiles::PublicProfile>, String> {
    Ok(profiles::list(&app_root(&app)?).map_err(|e| e.to_string())?.iter().map(Into::into).collect())
}

#[tauri::command]
pub(crate) fn profile_create(app: AppHandle, name: String, color: String) -> Result<profiles::PublicProfile, String> {
    Ok((&profiles::create(&app_root(&app)?, &name, &color)?).into())
}

/// Vérifie un code PIN ; chaque échec double presque l'attente suivante (1 s, 2 s… jusqu'à 5 s).
pub(crate) async fn check_pin(app: &AppHandle, id: &str, pin: Option<&str>) -> Result<(), String> {
    let result = profiles::verify(&app_root(app)?, id, pin);
    let state = app.state::<AppState>();
    let wait = {
        let mut failures = state.pin_failures.lock().map_err(|e| e.to_string())?;
        if result.is_ok() {
            failures.remove(id);
            0
        } else {
            let n = failures.entry(id.to_string()).or_insert(0);
            *n += 1;
            (*n).min(5)
        }
    };
    if wait > 0 {
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
    }
    result
}

#[tauri::command]
pub(crate) async fn profile_update(app: AppHandle, id: String, name: String, color: String, pin: Option<String>) -> Result<profiles::PublicProfile, String> {
    check_pin(&app, &id, pin.as_deref()).await?;
    Ok((&profiles::update(&app_root(&app)?, &id, &name, &color, pin.as_deref())?).into())
}

/// Définit, change (`new_pin`) ou retire (sans `new_pin`) le code d'un profil ; l'ancien code est exigé s'il y en a un.
#[tauri::command]
pub(crate) async fn profile_set_pin(app: AppHandle, id: String, new_pin: Option<String>, current_pin: Option<String>) -> Result<(), String> {
    check_pin(&app, &id, current_pin.as_deref()).await?;
    profiles::set_pin(&app_root(&app)?, &id, new_pin.as_deref().filter(|p| !p.trim().is_empty()), current_pin.as_deref())
}

/// Supprime un profil : ses données vont à la corbeille. Le profil ouvert et le dernier profil sont protégés.
#[tauri::command]
pub(crate) async fn profile_delete(app: AppHandle, state: State<'_, AppState>, id: String, pin: Option<String>) -> Result<(), String> {
    if state.profile.lock().map_err(|e| e.to_string())?.as_deref() == Some(id.as_str()) {
        return Err("Ferme ce profil avant de le supprimer.".into());
    }
    check_pin(&app, &id, pin.as_deref()).await?;
    profiles::remove(&app_root(&app)?, &id, pin.as_deref(), |dir| agent::fsops::to_trash(dir))?;
    // La clé du profil supprimé ne doit pas rester dans le coffre.
    let _ = secrets::set(&id, "api-key", "");
    Ok(())
}

/// Profil actuellement ouvert (une nouvelle fenêtre l'adopte sans redemander le code PIN).
#[tauri::command]
pub(crate) fn profile_active(app: AppHandle) -> Option<String> {
    active_profile(&app).ok()
}

/// Ouvre un profil : bascule l'historique, les connecteurs MCP, les skills et les agents sur ses données.
#[tauri::command]
pub(crate) async fn profile_select(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, id: String, pin: Option<String>) -> Result<(), String> {
    check_pin(&app, &id, pin.as_deref()).await?;
    if app.webview_windows().keys().filter(|l| !l.starts_with("pdf-")).count() > 1 {
        return Err("Ferme les autres fenêtres de Lama Desktop avant de changer de profil.".into());
    }
    if !state.running.lock().await.is_empty() {
        return Err("Une génération est en cours : arrête-la avant de changer de profil.".into());
    }
    let root = app_root(&app)?;
    profiles::find(&root, &id)?;
    let dir = profiles::data_dir(&root, &id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let history = History::open(&dir.join("history.db")).map_err(db_err)?;
    *db.lock().map_err(|e| e.to_string())? = history;
    *state.profile.lock().map_err(|e| e.to_string())? = Some(id.clone());
    invalidate_skills(&app);
    for agent in state.all_agents() {
        agent.changes.lock().await.clear();
        agent.approvals.lock().await.clear();
        agent.background.stop_all();
        agent.background.clear_finished();
    }
    *state.mcp.lock().await = Arc::new(mcp::McpHub::default());
    apply_pc(&load_pc(&dir));
    reminders::set_file(Some(dir.join("reminders.json")));
    schedules::set_file(Some(dir.join("schedules.json")));
    // Le coffre du profil précédent se referme : on ne garde jamais une clé d'un autre profil.
    vault::global().set_dir(Some(&dir));
    // Le navigateur de Lama appartient au profil : on ferme celui du profil précédent.
    browser::shutdown().await;
    browser::set_dir(Some(&dir));
    journal::set_file(Some(dir.join("actions.jsonl")));
    // Les connecteurs démarrent en arrière-plan : un `npx` ou `uvx` peut mettre une minute, l'ouverture du profil ne les attend pas.
    let (handle, config) = (app.clone(), dir.join("mcp.json"));
    tauri::async_runtime::spawn(async move {
        let hub = Arc::new(mcp::McpHub::start(&config, &id).await);
        let state = handle.state::<AppState>();
        // Si l'utilisateur a changé de profil entre-temps, ces connecteurs ne sont plus les bons.
        let still_open = state.profile.lock().ok().and_then(|p| p.clone()).as_deref() == Some(id.as_str());
        if still_open {
            *state.mcp.lock().await = hub;
            let _ = handle.emit("mcp:ready", ());
        }
    });
    Ok(())
}
