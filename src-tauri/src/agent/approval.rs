//! Quelles actions demandent l'accord de l'utilisateur selon le mode de permission.

use super::*;

/// Outils qui agissent sur le PC ou en lisent des données privées : toujours approuvés (sauf « Ignorer les permissions »).
pub(super) const PC_ACTIONS: &[&str] = &["open_target", "clipboard", "pc_control", "screenshot", "computer", "create_document", "browser_open", "browser_click", "browser_type"];

/// L'approbation est-elle nécessaire selon le mode de permission ?
pub fn needs_approval(mode: &str, prepared: &Prepared) -> bool {
    match (mode, prepared) {
        // Saisir un secret du coffre demande TOUJOURS l'accord, quel que soit le mode : c'est le seul garde-fou face à une injection.
        (_, Prepared::Meta { name, .. }) if name == "vault_fill" || name == "vault_login" => true,
        ("bypass", _) => false,
        (_, Prepared::Done(_)) => false,
        // Ouvrir un site ou une application sur le PC demande toujours l'accord (sauf « Ignorer les permissions »).
        (_, Prepared::Meta { name, .. }) if PC_ACTIONS.contains(&name.as_str()) => true,
        (_, Prepared::Meta { name, .. }) => mode == "manual" && !matches!(name.as_str(), "use_skill" | "search_skills" | "update_tasks" | "delegate" | "system_info" | "find_files" | "notify" | "reminders" | "web_search" | "remember" | "read_attachment" | "search_project_docs" | "search_chats" | "vault_list" | "browser_close"),
        (_, Prepared::Mcp { .. }) => true,
        // Une suppression demande toujours l'accord, même en « Auto » ou « Accepter les modifications ».
        (_, Prepared::Fs(op)) if op.is_delete() => true,
        ("accept-edits" | "auto", Prepared::Fs(_)) => false,
        ("accept-edits" | "auto", Prepared::Write { .. }) => false,
        ("auto", Prepared::Exec { command } | Prepared::Spawn { command }) => !tools::is_safe_command(command),
        _ => true,
    }
}
