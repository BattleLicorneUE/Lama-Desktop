//! Dossier de travail : fichiers, git, processus en arrière-plan, terminal, modèles de projet, exports.

use crate::*;

/// Tous les changements du dépôt, prêts à être relus par le modèle.
#[tauri::command]
pub(crate) fn git_review_diff(workspace: String) -> Result<String, String> {
    git::review_diff(std::path::Path::new(&workspace))
}

#[tauri::command]
pub(crate) fn git_branch_create(workspace: String, name: String) -> Result<String, String> {
    git::branch_create(std::path::Path::new(&workspace), &name)
}

#[tauri::command]
pub(crate) fn project_templates() -> Vec<templates::TemplateInfo> {
    templates::list()
}

#[tauri::command]
pub(crate) fn project_from_template(id: String, dir: String) -> Result<usize, String> {
    templates::create(&id, std::path::Path::new(&dir))
}

/// Règles du projet : le fichier AGENTS.md à la racine, lu par l'agent à chaque tâche.
#[tauri::command]
pub(crate) fn project_rules_get(workspace: String) -> Result<String, String> {
    let p = agent::sandbox::resolve(std::path::Path::new(&workspace), "AGENTS.md")?;
    Ok(std::fs::read_to_string(p).unwrap_or_default())
}

#[tauri::command]
pub(crate) fn project_rules_set(workspace: String, text: String) -> Result<(), String> {
    let p = agent::sandbox::resolve(std::path::Path::new(&workspace), "AGENTS.md")?;
    std::fs::write(p, text).map_err(|e| format!("Écriture impossible : {e}"))
}

/// Écrit un document (PDF, Word, HTML, Markdown selon l'extension) à partir de texte Markdown.
#[tauri::command]
pub(crate) async fn export_document(path: String, title: String, markdown: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || export::write(std::path::Path::new(&path), &title, &markdown)).await.map_err(|e| e.to_string())?
}

/// PDF d'une page HTML (artefact) : les scripts sont désactivés, la page est imprimée telle qu'elle s'affiche.
#[tauri::command]
pub(crate) async fn export_html_pdf(path: String, title: String, html: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let page = docs::static_page(&html);
        export::html_to_pdf(std::path::Path::new(&path), &title, &page)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub(crate) struct FsEntry {
    pub(crate) name: String,
    pub(crate) is_dir: bool,
}

/// Contenu d'un dossier du projet (pour l'arbre de fichiers).
#[tauri::command]
pub(crate) fn fs_list(workspace: String, rel: String) -> Result<Vec<FsEntry>, String> {
    const HIDDEN: [&str; 4] = [".git", "node_modules", "target", ".svelte-kit"];
    let dir = agent::sandbox::resolve(std::path::Path::new(&workspace), &rel)?;
    let mut out: Vec<FsEntry> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| FsEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        })
        .filter(|e| !HIDDEN.contains(&e.name.as_str()))
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// Arbre de fichiers : crée un fichier vide ou un dossier.
#[tauri::command]
pub(crate) fn fs_create(workspace: String, rel: String, is_dir: bool) -> Result<(), String> {
    agent::fsops::create(std::path::Path::new(&workspace), &rel, is_dir).map(|_| ())
}

/// Arbre de fichiers : renomme un élément (même dossier, nouveau nom).
#[tauri::command]
pub(crate) fn fs_rename(workspace: String, rel: String, new_name: String) -> Result<(), String> {
    let (from, to) = agent::fsops::rename_targets(std::path::Path::new(&workspace), &rel, &new_name)?;
    agent::fsops::do_move(&from, &to)
}

/// Arbre de fichiers : envoie un fichier ou un dossier à la corbeille.
#[tauri::command]
pub(crate) fn fs_delete(workspace: String, rel: String) -> Result<(), String> {
    let path = agent::fsops::existing_target(std::path::Path::new(&workspace), &rel)?;
    agent::fsops::to_trash(&path)
}

/// Enregistre un texte dans un fichier choisi par l'utilisateur (export de conversation).
#[tauri::command]
pub(crate) fn save_text(path: String, content: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Chemin vide.".into());
    }
    check_save_target(&path)?;
    std::fs::write(&path, content).map_err(|e| format!("Écriture impossible : {e}"))
}

/// État git du dossier de travail (branche, fichiers modifiés).
#[tauri::command]
pub(crate) fn git_status(workspace: String) -> Result<git::GitStatus, String> {
    git::status(std::path::Path::new(&workspace))
}

#[tauri::command]
pub(crate) fn git_diff(workspace: String, path: String) -> Result<String, String> {
    git::diff(std::path::Path::new(&workspace), &path)
}

/// Valide (commit) tous les changements. Ne pousse rien vers un serveur.
#[tauri::command]
pub(crate) fn git_commit(workspace: String, message: String) -> Result<String, String> {
    git::commit(std::path::Path::new(&workspace), &message)
}

/// Commandes lancées en arrière-plan par l'agent (état et fin de sortie).
#[tauri::command]
pub(crate) fn bg_list(window: tauri::Window, state: State<'_, AppState>) -> Vec<agent::background::ProcInfo> {
    state.agent_of(window.label()).background.list()
}

#[tauri::command]
pub(crate) fn bg_stop(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.agent_of(window.label()).background.stop(&id)
}

#[tauri::command]
pub(crate) fn bg_clear(window: tauri::Window, state: State<'_, AppState>) {
    state.agent_of(window.label()).background.clear_finished();
}

/// Console du mode Code : exécute une commande dans le dossier du projet.
#[tauri::command]
pub(crate) async fn term_run(workspace: String, command: String) -> Result<String, String> {
    agent::tools::exec_command(std::path::Path::new(&workspace), &command).await
}

#[derive(serde::Serialize)]
pub(crate) struct Attachment {
    pub(crate) name: String,
    /// "image" (contenu en base64) ou "text".
    pub(crate) kind: String,
    pub(crate) data: String,
}

/// Lit un fichier déposé ou choisi par l'utilisateur pour l'attacher à un message.
#[tauri::command]
pub(crate) fn read_attachment(path: String) -> Result<Attachment, String> {
    use base64::Engine;
    const MAX_TEXT: u64 = 300_000;
    const MAX_IMAGE: u64 = 15_000_000;
    let p = std::path::Path::new(&path);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let size = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
    if ["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str()) {
        if size > MAX_IMAGE {
            return Err(format!("Image trop volumineuse ({} Mo max)", MAX_IMAGE / 1_000_000));
        }
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        return Ok(Attachment { name, kind: "image".into(), data: base64::engine::general_purpose::STANDARD.encode(bytes) });
    }
    if ext == "pdf" {
        let data = pdf::extract_text(p)?;
        return Ok(Attachment { name, kind: "text".into(), data });
    }
    if size > MAX_TEXT {
        return Err(format!("Fichier trop volumineux ({} Ko max)", MAX_TEXT / 1000));
    }
    let data = std::fs::read_to_string(p).map_err(|_| format!("{name} n'est pas un fichier texte lisible"))?;
    // Moins de jetons pour rien : espaces de fin et longues séries de lignes vides retirés.
    Ok(Attachment { name, kind: "text".into(), data: pdf::tidy(&data) })
}
