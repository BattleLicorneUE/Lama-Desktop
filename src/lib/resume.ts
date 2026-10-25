/** Reprise d'une tâche interrompue (bouton Stop, arrêt d'urgence, erreur, fermeture de l'application). */

type ToolLike = { name: string; args: Record<string, unknown>; state: string; output?: string | null; agent?: string | null };

const target = (t: ToolLike): string => {
  const a = t.args;
  const v = t.name === "move_path" ? `${a.from} → ${a.to}` : (a.path ?? a.command ?? a.pattern ?? a.target ?? a.query ?? a.url ?? a.name ?? a.prompt ?? a.agent ?? "");
  return String(v).replace(/\s+/g, " ").slice(0, 110);
};

const STATE: Record<string, string> = { done: "fait", error: "échec", denied: "refusé ou interrompu", awaiting: "interrompu", running: "interrompu", undone: "annulé" };

/**
 * Note donnée au modèle pour qu'il reprenne là où il s'est arrêté : ce qui est fait, ce qui ne l'est pas,
 * et le texte déjà écrit. Les lectures ne sont pas listées (inutile de les refaire de mémoire, le modèle relit au besoin).
 */
export function resumeNote(msg: { content: string; tools?: ToolLike[] }): string {
  const acts = (msg.tools ?? []).filter((t) => !["read_file", "list_dir", "grep", "find_symbol", "use_skill", "search_skills"].includes(t.name));
  const lines = acts.slice(-40).map((t) => {
    const out = t.state === "error" && t.output ? ` — ${t.output.replace(/\s+/g, " ").slice(0, 120)}` : "";
    return `- ${t.agent ? t.agent + " · " : ""}${t.name} ${target(t)} : ${STATE[t.state] ?? t.state}${out}`;
  });
  const partial = msg.content.trim();
  return [
    "TÂCHE INTERROMPUE : ta réponse précédente a été coupée avant la fin (arrêt, erreur ou fermeture). Reprends exactement là où tu t'étais arrêté.",
    lines.length ? `Actions déjà tentées :\n${lines.join("\n")}` : "Aucune action n'avait encore été faite.",
    partial ? `Texte déjà écrit (ne le répète pas) :\n${partial.slice(0, 1500)}` : "",
    "Consignes : ne refais pas ce qui est marqué « fait » (vérifie l'état réel des fichiers si besoin) ; reprends les actions « interrompu » ou « refusé » seulement si elles sont toujours nécessaires ; termine la tâche, puis résume ce qui a été fait.",
  ]
    .filter(Boolean)
    .join("\n\n");
}
