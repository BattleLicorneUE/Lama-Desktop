/** Modèles de départ de l'éditeur d'agents, et rédaction assistée par le modèle. */

export type AgentDraft = {
  name: string;
  description: string;
  prompt: string;
  model: string;
  tools: string[] | null;
};

export const READ_TOOLS = ["read_file", "list_dir", "grep"];
export const WEB_TOOLS = [...READ_TOOLS, "web_search", "web_fetch"];

export type Access = "all" | "read" | "web" | "custom";

export const ACCESS: { id: Exclude<Access, "custom">; label: string; help: string; tools: string[] | null }[] = [
  { id: "all", label: "Tout", help: "Lire, modifier des fichiers, lancer des commandes et chercher sur le web (avec ton accord, comme d'habitude).", tools: null },
  { id: "read", label: "Lecture seule", help: "Lire, chercher et lister des fichiers. Rien ne peut être modifié.", tools: READ_TOOLS },
  { id: "web", label: "Lecture et web", help: "Comme la lecture seule, avec la recherche sur internet.", tools: WEB_TOOLS },
];

const same = (a: string[], b: string[]) => a.length === b.length && a.every((t) => b.includes(t));

/** À quel réglage de l'éditeur correspond une liste d'outils. */
export function accessOf(tools: string[] | null): Access {
  if (!tools) return "all";
  if (same(tools, READ_TOOLS)) return "read";
  if (same(tools, WEB_TOOLS)) return "web";
  return "custom";
}

export type AgentTemplate = { id: string; label: string; draft: Omit<AgentDraft, "model"> };

export const AGENT_TEMPLATES: AgentTemplate[] = [
  { id: "blank", label: "Page blanche", draft: { name: "", description: "", prompt: "Tu es …\n\nTa méthode :\n1. \n2. \n\nTa réponse : …", tools: null } },
  {
    id: "securite",
    label: "Relecteur sécurité",
    draft: {
      name: "relecteur-securite",
      description: "Confie-lui une relecture de code axée sécurité : failles, secrets oubliés, entrées non vérifiées. Il ne modifie rien.",
      prompt: "Tu es un relecteur spécialisé en sécurité. Lis les fichiers concernés puis liste les problèmes par ordre de gravité (injection, secrets dans le code, entrées non vérifiées, droits trop larges), chacun avec le fichier, la ligne et une correction proposée. Si tout est correct, dis-le simplement. Ne modifie rien.",
      tools: READ_TOOLS,
    },
  },
  {
    id: "redacteur",
    label: "Rédacteur",
    draft: {
      name: "redacteur",
      description: "Confie-lui la rédaction d'un texte (mail, article, fiche) à partir de notes ou d'un brief.",
      prompt: "Tu es un rédacteur clair et concret. À partir du brief, écris un texte prêt à l'emploi : phrases courtes, vocabulaire simple, ton adapté au destinataire. Si une information manque, pose une seule question avant d'écrire.",
      tools: READ_TOOLS,
    },
  },
  {
    id: "veille",
    label: "Veilleur web",
    draft: {
      name: "veilleur",
      description: "Confie-lui une recherche sur internet : il rapporte des faits datés avec leurs sources.",
      prompt: "Tu es un veilleur méthodique. Fais plusieurs recherches avec des formulations différentes, ouvre les pages les plus fiables et relève les faits utiles. Réponds par une synthèse courte, chaque fait suivi de son adresse. Signale les contradictions et ce que tu n'as pas trouvé. N'invente rien.",
      tools: WEB_TOOLS,
    },
  },
];

export const AGENT_WRITER_PROMPT = `Tu aides à écrire des « agents » pour un assistant IA : des rôles spécialisés auxquels l'assistant confie une partie d'un travail.
À partir de l'idée de l'utilisateur, écris l'agent en français.
Réponds uniquement avec un objet JSON valide, sans texte autour, de cette forme :
{"name":"...","description":"...","prompt":"..."}
- "name" : 1 à 3 mots en minuscules séparés par des tirets (ex. relecteur-securite).
- "description" : une ou deux phrases qui disent QUAND confier une tâche à cet agent. C'est ce qui permet à l'assistant de le choisir.
- "prompt" : la consigne de l'agent, à la deuxième personne : son rôle, sa méthode en quelques étapes, le format de sa réponse. Concret et court.`;

/** Lit la réponse du modèle : un objet JSON, même entouré de texte ou de ```. */
export function parseAgentReply(text: string): Partial<AgentDraft> | null {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    const o = JSON.parse(text.slice(start, end + 1));
    if (typeof o !== "object" || !o) return null;
    const str = (v: unknown) => (typeof v === "string" ? v : "");
    const d = { name: str(o.name), description: str(o.description), prompt: str(o.prompt) };
    return d.description || d.prompt ? d : null;
  } catch {
    return null;
  }
}
