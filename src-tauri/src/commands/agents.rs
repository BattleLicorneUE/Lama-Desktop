//! Agents personnalisés.

use crate::*;

pub(crate) fn agents_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(profile_dir(app)?.join("agents"))
}

/// Agents disponibles : intégrés, ceux de l'application et ceux du projet (`.agents/`).
#[tauri::command]
pub(crate) fn agents_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<agents::AgentProfile>, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    Ok(agents::scan(&agents_dir(&app)?, ws.as_deref()))
}

/// Un agent ouvert dans l'éditeur : son contenu et sa portée.
#[derive(serde::Serialize)]
pub(crate) struct AgentEdit {
    pub(crate) draft: agents::Draft,
    pub(crate) scope: String,
}

pub(crate) fn find_agent(app: &AppHandle, workspace: Option<String>, name: &str) -> Result<agents::AgentProfile, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    let all = agents::scan(&agents_dir(app)?, ws.as_deref());
    agents::find(&all, name).cloned().ok_or_else(|| "Agent introuvable.".to_string())
}

/// Contenu d'un agent pour l'éditeur (les agents intégrés peuvent être copiés, pas modifiés sur place).
#[tauri::command]
pub(crate) fn agent_get(app: AppHandle, workspace: Option<String>, name: String) -> Result<AgentEdit, String> {
    let a = find_agent(&app, workspace, &name)?;
    Ok(AgentEdit { draft: agents::draft_of(&a), scope: a.scope })
}

/// Crée ou modifie un agent (`original` : le nom de l'agent modifié). `scope` : « app » ou « project » (`.agents/` du dossier de travail).
#[tauri::command]
pub(crate) fn agent_save(app: AppHandle, workspace: Option<String>, draft: agents::Draft, scope: String, original: Option<String>) -> Result<String, String> {
    let existing = match original.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => {
            let a = find_agent(&app, workspace.clone(), n)?;
            if a.file.is_none() {
                return Err("Un agent intégré ne se modifie pas : enregistre-le sous un autre nom pour en faire ta version.".into());
            }
            Some(a)
        }
        None => None,
    };
    let dir = match &existing {
        Some(a) => a.file.as_ref().and_then(|f| f.parent()).map(std::path::Path::to_path_buf).ok_or("Dossier de l'agent introuvable.")?,
        None if scope == "project" => {
            let ws = workspace.filter(|w| !w.trim().is_empty()).ok_or("Ouvre d'abord un dossier de travail pour créer un agent de projet.")?;
            std::path::PathBuf::from(ws).join(".agents")
        }
        None => agents_dir(&app)?,
    };
    agents::save(&dir, &draft, existing.as_ref().and_then(|a| a.file.as_deref())).map(|p| p.to_string_lossy().into_owned())
}

/// Supprime un agent (jamais un agent intégré).
#[tauri::command]
pub(crate) fn agent_delete(app: AppHandle, workspace: Option<String>, name: String) -> Result<(), String> {
    let a = find_agent(&app, workspace, &name)?;
    let file = a.file.ok_or("Un agent intégré ne se supprime pas.")?;
    let dir = file.parent().map(std::path::Path::to_path_buf).ok_or("Dossier de l'agent introuvable.")?;
    agents::delete(&dir, &file)
}

/// Copie les agents de Claude Code (`~/.claude/agents`) dans l'application. Renvoie le nombre importé.
#[tauri::command]
pub(crate) fn agents_import_claude(app: AppHandle) -> Result<usize, String> {
    let home = std::env::var_os("USERPROFILE").ok_or("Dossier utilisateur introuvable")?;
    agents::import_from(&std::path::PathBuf::from(home).join(".claude").join("agents"), &agents_dir(&app)?)
}

#[tauri::command]
pub(crate) fn agents_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = agents_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}
