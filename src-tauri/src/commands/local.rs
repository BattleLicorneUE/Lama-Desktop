//! Modèles locaux (llama.cpp), détection du PC et état de la connexion.

use crate::*;

/// Modèles locaux installés sur ce PC.
#[tauri::command]
pub(crate) async fn local_installed(app: AppHandle) -> Result<Vec<localai::Installed>, String> {
    Ok(localai::installed(&app_root(&app)?, &hardware::cached().await))
}

/// Taille du contexte d'un modèle local (0 = automatique) ; appliquée au prochain message.
#[tauri::command]
pub(crate) fn local_set_ctx(app: AppHandle, id: String, ctx: u32) -> Result<(), String> {
    localai::set_ctx_setting(&app_root(&app)?, &id, ctx)
}

#[derive(serde::Serialize)]
pub(crate) struct LocalStatus {
    pub(crate) running: Option<String>,
    pub(crate) installing: Option<String>,
    pub(crate) engine_bytes: u64,
    /// Espace pris par les téléchargements interrompus.
    pub(crate) partial_bytes: u64,
    /// Espace libre sur le disque des données de l'application, en Mo.
    pub(crate) free_mb: u64,
}

#[tauri::command]
pub(crate) async fn local_status(app: AppHandle) -> Result<LocalStatus, String> {
    let root = app_root(&app)?;
    Ok(LocalStatus { running: localai::running_model().await, installing: localai::installing(), engine_bytes: localai::engine_bytes(&root), partial_bytes: localai::partial_bytes(&root), free_mb: hardware::free_mb(&root) })
}

/// Lance l'installation d'un modèle du catalogue : la progression arrive par `local:progress`, la fin par `local:done` ou `local:error`.
#[tauri::command]
pub(crate) async fn local_install(app: AppHandle, id: String) -> Result<(), String> {
    let model = localmodels::catalog().into_iter().find(|m| m.id == id).ok_or("Modèle inconnu.")?;
    if let Some(other) = localai::installing() {
        return Err(format!("Une installation est déjà en cours ({other})."));
    }
    let data = app_root(&app)?;
    tokio::spawn(async move {
        let hw = tokio::task::spawn_blocking(hardware::detect).await.ok();
        let outcome = match hw {
            Some(hw) => {
                let a = app.clone();
                localai::install(&data, &model, &hw, std::sync::Arc::new(move |p| { let _ = a.emit("local:progress", p); })).await
            }
            None => Err("Analyse du PC impossible.".to_string()),
        };
        match outcome {
            Ok(()) => {
                track(&app, "local_model_installed", serde_json::json!({ "model": model.id }));
                let _ = app.emit("local:done", serde_json::json!({ "id": model.id }));
            }
            Err(e) => {
                track(&app, "local_model_failed", serde_json::json!({ "model": model.id }));
                let _ = app.emit("local:error", serde_json::json!({ "id": model.id, "error": e }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn local_cancel(id: String) {
    localai::cancel(&id);
}

#[tauri::command]
pub(crate) async fn local_remove(app: AppHandle, id: String) -> Result<u64, String> {
    localai::remove(&app_root(&app)?, &id).await
}

/// Supprime les téléchargements interrompus ; renvoie l'espace libéré en octets.
#[tauri::command]
pub(crate) fn local_clean_partials(app: AppHandle) -> Result<u64, String> {
    localai::clean_partials(&app_root(&app)?)
}

/// Mémoire, carte graphique, processeur et disque de ce PC, pour recommander des modèles locaux.
#[tauri::command]
pub(crate) async fn hardware_detect() -> Result<hardware::Hardware, String> {
    tokio::task::spawn_blocking(hardware::detect).await.map_err(|e| e.to_string())
}

/// L'ordinateur a-t-il internet ? Même vérification que Windows ; elle sert à couper la recherche web quand on est hors ligne :
/// un Wi-Fi sans accès à internet est « connecté » pour Windows, mais pas pour une recherche.
#[tauri::command]
pub(crate) async fn net_online() -> bool {
    web::online().await
}

/// Les modèles locaux du catalogue, jugés pour la machine décrite par `hardware` (le résultat de `hardware_detect`).
#[tauri::command]
pub(crate) fn local_models_assess(hardware: hardware::Hardware) -> Vec<localmodels::Assessed> {
    localmodels::assess_all(&hardware)
}
