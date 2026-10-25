/** Nom lisible de chaque outil de l'agent. */
export const LABELS: Record<string, string> = {
  read_file: "Lecture", list_dir: "Dossier", grep: "Recherche",
  write_file: "Écriture", edit_file: "Modification", make_dir: "Nouveau dossier", move_path: "Déplacement", delete_path: "Suppression", run_command: "Commande", verify: "Vérification", use_skill: "Skill", delegate: "Délégation", command_output: "Sortie", stop_command: "Arrêt", search_skills: "Recherche de skills", web_search: "Recherche web", web_fetch: "Page web", system_info: "Infos du PC", open_target: "Ouverture", clipboard: "Presse-papiers", pc_control: "Réglage du PC", screenshot: "Capture d'écran", computer: "Souris / clavier", find_files: "Recherche de fichiers", notify: "Notification", create_document: "Document", reminders: "Rappels", vault_list: "Coffre", vault_fill: "Saisie du coffre", vault_login: "Connexion (coffre)", browser_open: "Navigateur : ouvrir", browser_read: "Navigateur : lire", browser_click: "Navigateur : clic", browser_type: "Navigateur : saisie", browser_close: "Navigateur : fermer",
};

export function toolLabel(name: string): string {
  return LABELS[name] ?? (name.startsWith("mcp__") ? "MCP · " + name.split("__").slice(1).join(" / ") : name);
}
