//! Conversations, projets, mémoire, documents et artéfacts (base SQLite).

use crate::*;

#[tauri::command]
pub(crate) fn memory_list(db: Db) -> Result<Vec<Memory>, String> {
    db.lock_safe().list_memories().map_err(db_err)
}

#[tauri::command]
pub(crate) fn memory_add(db: Db, text: String, project_id: Option<String>) -> Result<Memory, String> {
    db.lock_safe().add_memory_in(text.trim(), project_id.as_deref().filter(|p| !p.is_empty())).map_err(db_err)
}

/// Mémoire propre à un projet.
#[tauri::command]
pub(crate) fn memory_project_list(db: Db, project_id: String) -> Result<Vec<Memory>, String> {
    db.lock_safe().list_memories_of(Some(&project_id)).map_err(db_err)
}

#[tauri::command]
pub(crate) fn memory_delete(db: Db, id: i64) -> Result<(), String> {
    db.lock_safe().delete_memory(id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_docs(db: Db, project_id: String) -> Result<Vec<ProjectDoc>, String> {
    db.lock_safe().list_docs(&project_id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_doc_add(db: Db, project_id: String, name: String, content: String) -> Result<ProjectDoc, String> {
    db.lock_safe().add_doc(&project_id, &name, &content).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_doc_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_doc(&id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_create(
    db: Db,
    model: String,
    kind: String,
    workspace: Option<String>,
    project_id: Option<String>,
) -> Result<Conversation, String> {
    db.lock_safe().create(&model, &kind, workspace.as_deref(), project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_list(db: Db, search: String, kind: String, project_id: Option<String>) -> Result<Vec<Conversation>, String> {
    db.lock_safe().list(&search, &kind, project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_create(db: Db, name: String, instructions: String, kind: Option<String>, workspace: Option<String>) -> Result<Project, String> {
    let kind = if kind.as_deref() == Some("code") { "code" } else { "chat" };
    db.lock_safe().create_project(&name, &instructions, kind, workspace.as_deref().filter(|w| !w.trim().is_empty())).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_list(db: Db) -> Result<Vec<Project>, String> {
    db.lock_safe().list_projects().map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_update(db: Db, id: String, name: String, instructions: String, workspace: Option<String>) -> Result<(), String> {
    db.lock_safe().update_project(&id, &name, &instructions, workspace.as_deref().filter(|w| !w.trim().is_empty())).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_project(&id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn artifact_save(db: Db, conv_id: Option<String>, title: String, lang: String, kind: String, code: String) -> Result<history::SavedArtifact, String> {
    db.lock_safe().save_artifact(conv_id.as_deref(), &title, &lang, &kind, &code).map_err(db_err)
}

#[tauri::command]
pub(crate) fn artifact_list(db: Db, search: String) -> Result<Vec<history::SavedArtifact>, String> {
    db.lock_safe().list_artifacts(&search).map_err(db_err)
}

#[tauri::command]
pub(crate) fn artifact_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_artifact(&id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn project_pin(db: Db, id: String, pinned: bool) -> Result<(), String> {
    db.lock_safe().set_project_pinned(&id, pinned).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_pin(db: Db, id: String, pinned: bool) -> Result<(), String> {
    db.lock_safe().set_pinned(&id, pinned).map_err(db_err)
}

/// Nouvelle conversation = copie de celle-ci jusqu'au message `upto` (exclu).
#[tauri::command]
pub(crate) fn conv_fork(db: Db, id: String, upto: usize) -> Result<Conversation, String> {
    db.lock_safe().fork(&id, upto).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_set_project(db: Db, id: String, project_id: Option<String>) -> Result<(), String> {
    db.lock_safe().set_conversation_project(&id, project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_messages(db: Db, id: String) -> Result<Vec<StoredMessage>, String> {
    db.lock_safe().messages(&id).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_save(db: Db, id: String, model: String, messages: Vec<StoredMessage>) -> Result<(), String> {
    db.lock_safe().save_messages(&id, &model, &messages).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_rename(db: Db, id: String, title: String) -> Result<(), String> {
    db.lock_safe().rename(&id, &title).map_err(db_err)
}

#[tauri::command]
pub(crate) fn conv_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete(&id).map_err(db_err)
}
