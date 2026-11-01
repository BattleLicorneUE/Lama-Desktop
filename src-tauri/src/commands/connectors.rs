//! Connecteurs et serveurs MCP.

use crate::*;

#[tauri::command]
pub(crate) fn connectors_catalog() -> Vec<connectors::Connector> {
    connectors::catalog()
}

/// Identifiants des serveurs déjà présents dans `mcp.json`.
#[tauri::command]
pub(crate) fn connectors_installed(app: AppHandle) -> Result<Vec<String>, String> {
    let path = mcp_config_file(app)?;
    let cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Ok(cfg["mcpServers"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default())
}

/// Ajoute un connecteur du catalogue : jetons dans le coffre Windows, renvois dans `mcp.json`.
#[tauri::command]
pub(crate) fn connector_add(app: AppHandle, id: String, values: HashMap<String, String>) -> Result<(), String> {
    let conn = connectors::catalog().into_iter().find(|c| c.id == id).ok_or("Connecteur inconnu.")?;
    let (entry, secrets_to_store) = connectors::build_entry(&conn, &values)?;
    let profile = active_profile(&app)?;
    for (name, value) in &secrets_to_store {
        secrets::set(&profile, name, value)?;
    }
    let path = mcp_config_file(app.clone())?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg.as_object_mut().ok_or("mcp.json invalide")?.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
    servers[&id] = entry;
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    track(&app, "connector_added", serde_json::json!({ "connector": id }));
    Ok(())
}

/// Réécrit `mcp.json` après modification de la liste des serveurs.
pub(crate) fn mcp_update(app: &AppHandle, change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<(), String>) -> Result<(), String> {
    let path = mcp_config_file(app.clone())?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg.as_object_mut().ok_or("mcp.json invalide")?.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
    change(servers.as_object_mut().ok_or("mcp.json invalide")?)?;
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Ajoute un serveur MCP personnalisé : une commande à lancer sur ce PC, ou l'adresse d'un serveur HTTP.
/// `pairs` : variables d'environnement (`NOM=valeur`) ou en-têtes (`Nom: valeur`), une par ligne.
#[tauri::command]
pub(crate) fn mcp_add_custom(app: AppHandle, name: String, http: bool, target: String, pairs: String, all_secret: bool) -> Result<String, String> {
    let id = connectors::clean_id(&name)?;
    let (entry, secrets_to_store) = connectors::custom_entry(&id, http, &target, &connectors::parse_pairs(&pairs, http)?, all_secret)?;
    let profile = active_profile(&app)?;
    mcp_update(&app, |servers| {
        if servers.contains_key(&id) {
            return Err(format!("« {id} » existe déjà : retire-le d'abord ou choisis un autre nom."));
        }
        for (n, v) in &secrets_to_store {
            secrets::set(&profile, n, v)?;
        }
        servers.insert(id.clone(), entry);
        Ok(())
    })?;
    Ok(id)
}

/// Importe une configuration collée (`{ "mcpServers": { … } }`) ; renvoie les noms ajoutés.
#[tauri::command]
pub(crate) fn mcp_import(app: AppHandle, text: String) -> Result<Vec<String>, String> {
    let list = connectors::import_servers(&text)?;
    let profile = active_profile(&app)?;
    let mut added = Vec::new();
    mcp_update(&app, |servers| {
        if let Some((id, _, _)) = list.iter().find(|(id, _, _)| servers.contains_key(id)) {
            return Err(format!("« {id} » existe déjà : retire-le d'abord."));
        }
        for (id, entry, secs) in list {
            for (n, v) in &secs {
                secrets::set(&profile, n, v)?;
            }
            servers.insert(id.clone(), entry);
            added.push(id);
        }
        Ok(())
    })?;
    Ok(added)
}

/// Retire un serveur MCP (du catalogue ou personnalisé) et efface les secrets qu'il citait.
#[tauri::command]
pub(crate) fn mcp_remove(app: AppHandle, name: String) -> Result<(), String> {
    let profile = active_profile(&app)?;
    mcp_update(&app, |servers| {
        if let Some(entry) = servers.remove(&name) {
            for n in connectors::secret_names(&entry) {
                let _ = secrets::set(&profile, &n, "");
            }
        }
        Ok(())
    })
}

/// Retire un connecteur et efface ses secrets du coffre.
#[tauri::command]
pub(crate) fn connector_remove(app: AppHandle, id: String) -> Result<(), String> {
    let profile = active_profile(&app)?;
    if let Some(conn) = connectors::catalog().into_iter().find(|c| c.id == id) {
        for fld in conn.fields.iter().filter(|f| f.secret) {
            let _ = secrets::set(&profile, &format!("mcp:{}:{}", conn.id, fld.key), "");
        }
    }
    let path = mcp_config_file(app)?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Some(o) = cfg.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        o.remove(&id);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Ajoute un serveur MCP à mcp.json (sans toucher aux autres).
#[tauri::command]
pub(crate) fn mcp_add(app: AppHandle, name: String, command: String, args: Vec<String>) -> Result<(), String> {
    let path = mcp_config_file(app)?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg
        .as_object_mut()
        .ok_or("mcp.json invalide")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    servers[name] = serde_json::json!({ "command": command, "args": args });
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn mcp_status(state: State<'_, AppState>) -> Result<Vec<mcp::ServerStatus>, String> {
    Ok(state.mcp.lock().await.status.clone())
}

pub(crate) fn mcp_config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(profile_dir(app)?.join("mcp.json"))
}

#[tauri::command]
pub(crate) fn mcp_config_file(app: AppHandle) -> Result<String, String> {
    let path = mcp_config_path(&app)?;
    if !path.exists() {
        std::fs::write(&path, "{
  \"mcpServers\": {}
}
").map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Ouvre mcp.json dans le Bloc-notes pour l'éditer.
#[tauri::command]
pub(crate) fn mcp_edit_config(app: AppHandle) -> Result<(), String> {
    let path = mcp_config_file(app)?;
    std::process::Command::new("notepad").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn mcp_reload(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<mcp::ServerStatus>, String> {
    let hub = Arc::new(mcp::McpHub::start(&mcp_config_path(&app)?, &active_profile(&app)?).await);
    let status = hub.status.clone();
    *state.mcp.lock().await = hub;
    Ok(status)
}
