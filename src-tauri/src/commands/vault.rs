//! Coffre de mots de passe et navigateur de Lama.

use crate::*;

#[tauri::command]
pub(crate) fn vault_status() -> vault::Status {
    vault::global().status()
}

#[tauri::command]
pub(crate) async fn vault_create(master: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().create(&master)).await.map_err(|e| e.to_string())?
}

/// Ouvre le coffre. Chaque échec rallonge l'attente suivante (jusqu'à 5 s) : deviner le mot de passe à la main prend des heures.
#[tauri::command]
pub(crate) async fn vault_unlock(master: String) -> Result<(), String> {
    let r = tokio::task::spawn_blocking(move || vault::global().unlock(&master)).await.map_err(|e| e.to_string())?;
    if r.is_err() {
        tokio::time::sleep(std::time::Duration::from_secs(vault::global().failures().min(5) as u64)).await;
    }
    r
}

#[tauri::command]
pub(crate) fn vault_lock() {
    vault::global().lock();
}

#[tauri::command]
pub(crate) fn vault_list() -> Result<serde_json::Value, String> {
    let (logins, infos) = vault::global().list()?;
    Ok(serde_json::json!({ "logins": logins, "infos": infos }))
}

#[tauri::command]
pub(crate) fn vault_login_get(id: String) -> Result<vault::Login, String> {
    vault::global().get_login(&id)
}

#[tauri::command]
pub(crate) fn vault_login_save(login: vault::Login) -> Result<vault::Login, String> {
    vault::global().save_login(login)
}

#[tauri::command]
pub(crate) fn vault_login_delete(id: String) -> Result<(), String> {
    vault::global().delete_login(&id)
}

#[tauri::command]
pub(crate) fn vault_info_save(info: vault::Info) -> Result<vault::Info, String> {
    vault::global().save_info(info)
}

#[tauri::command]
pub(crate) fn vault_info_delete(id: String) -> Result<(), String> {
    vault::global().delete_info(&id)
}

#[tauri::command]
pub(crate) fn vault_generate(opts: vault::GenOpts) -> Result<serde_json::Value, String> {
    let (password, bits) = vault::generate(&opts)?;
    Ok(serde_json::json!({ "password": password, "bits": bits }))
}

#[tauri::command]
pub(crate) fn vault_set_agent_access(on: bool) -> Result<(), String> {
    vault::global().set_agent_access(on)
}

#[tauri::command]
pub(crate) async fn vault_change_master(old: String, new: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().change_master(&old, &new)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn vault_destroy(master: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().destroy(&master)).await.map_err(|e| e.to_string())?
}

/// Copie un secret ; le presse-papiers est vidé au bout de 30 s s'il le contient encore. Le secret ne revient jamais à l'interface.
#[tauri::command]
pub(crate) async fn vault_copy(id: String, field: String) -> Result<u64, String> {
    const SECONDS: u64 = 30;
    let secret = vault::global().secret(&id, &field)?;
    winio::copy_secret(&secret)?;
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(SECONDS)).await;
        winio::clear_clipboard_if(&secret);
    });
    Ok(SECONDS)
}

/// Saisie demandée par l'utilisateur lui-même (bouton « Remplir ») : `{ items: [{ kind, id }], wait, submit, tab }`.
#[tauri::command]
pub(crate) async fn vault_fill_ui(app: AppHandle, resolved: serde_json::Value) -> Result<String, String> {
    vault::fill_flow(&app, &resolved).await
}

#[tauri::command]
pub(crate) async fn browser_status() -> serde_json::Value {
    let found = browser::find_browser();
    serde_json::json!({
        "available": found.is_ok(),
        "name": found.as_ref().map(|(_, n)| *n).unwrap_or(""),
        "running": browser::is_running().await.is_some(),
    })
}

/// Ouvre le navigateur de Lama (pour s'y connecter à la main : double authentification, CAPTCHA) sur une adresse facultative.
#[tauri::command]
pub(crate) async fn browser_launch_ui(url: Option<String>) -> Result<String, String> {
    browser::open(url.as_deref().filter(|u| !u.trim().is_empty()).unwrap_or("about:blank").replace("about:blank", "https://duckduckgo.com/").as_str()).await
}

#[tauri::command]
pub(crate) async fn browser_close_ui() -> Result<String, String> {
    browser::close().await
}

/// Efface les cookies, sessions et le cache du navigateur de Lama (déconnecte de tous les sites).
#[tauri::command]
pub(crate) async fn browser_wipe(app: AppHandle) -> Result<(), String> {
    browser::shutdown().await;
    let dir = profile_dir(&app)?.join("browser");
    tokio::task::spawn_blocking(move || if dir.exists() { std::fs::remove_dir_all(dir).map_err(|e| format!("Suppression impossible : {e}")) } else { Ok(()) })
        .await
        .map_err(|e| e.to_string())?
}

/// « Se connecter » depuis le coffre : ouvre le site de l'entrée dans le navigateur de Lama, puis y saisit identifiant et mot de passe.
#[tauri::command]
pub(crate) async fn vault_browser_login(id: String, submit: bool) -> Result<String, String> {
    let login = vault::global().get_login(&id)?;
    if login.url.trim().is_empty() {
        return Err("Cette entrée n'a pas d'adresse de site : ajoute-la pour te connecter automatiquement.".into());
    }
    let url = if login.url.contains("://") { login.url.clone() } else { format!("https://{}", login.url) };
    browser::open(&url).await?;
    vault::browser_login(&serde_json::json!({ "id": id, "mode": "login", "submit": submit, "label": login.title })).await
}
