/** Modèles de départ de l'éditeur de skills, et rédaction assistée par le modèle. */

export type SkillDraft = {
  name: string;
  description: string;
  body: string;
  argument_hint: string;
  model_invocable: boolean;
};

export type SkillTemplate = { id: string; label: string; draft: Omit<SkillDraft, "model_invocable"> };

export const SKILL_TEMPLATES: SkillTemplate[] = [
  {
    id: "blank",
    label: "Page blanche",
    draft: {
      name: "",
      description: "",
      argument_hint: "",
      body: "# Objectif\n\nCe que Lama doit faire avec ce skill.\n\n# Étapes\n\n1. \n2. \n3. \n\n# Format de la réponse\n\nComment présenter le résultat.\n",
    },
  },
  {
    id: "relire",
    label: "Relire un texte",
    draft: {
      name: "relire-un-texte",
      description: "Utilise ce skill quand on demande de relire, corriger ou améliorer un texte (fautes, clarté, ton).",
      argument_hint: "[le texte à relire]",
      body: "# Relecture\n\nTexte à relire : $ARGUMENTS\n\n1. Corrige l'orthographe, la grammaire et la ponctuation sans changer le sens.\n2. Garde le ton et le vocabulaire de l'auteur : ne réécris pas ce qui est déjà bon.\n3. Signale en deux ou trois points ce qui pourrait être plus clair.\n\n# Format de la réponse\n\nD'abord le texte corrigé, puis la liste courte de tes remarques.\n",
    },
  },
  {
    id: "resumer",
    label: "Résumer un document",
    draft: {
      name: "resumer",
      description: "Utilise ce skill quand on demande un résumé, une synthèse ou les points importants d'un texte ou d'un document.",
      argument_hint: "[le texte ou le fichier]",
      body: "# Résumé\n\nÀ résumer : $ARGUMENTS\n\n1. Lis tout avant d'écrire.\n2. Donne le résumé en 3 à 5 phrases.\n3. Termine par les points à retenir, sous forme de liste, et les questions restées ouvertes s'il y en a.\n\nN'invente rien : si une information manque, dis-le.\n",
    },
  },
  {
    id: "commit",
    label: "Message de commit",
    draft: {
      name: "message-de-commit",
      description: "Utilise ce skill quand on demande d'écrire un message de commit à partir des changements en cours.",
      argument_hint: "",
      body: "# Message de commit\n\n1. Regarde les changements (git status, puis git diff).\n2. Écris une première ligne de 60 caractères au plus, à l'impératif (« Corrige… », « Ajoute… »).\n3. Ajoute, si utile, un court paragraphe qui explique le pourquoi plus que le comment.\n\nPropose le message sans lancer le commit.\n",
    },
  },
  {
    id: "expliquer",
    label: "Expliquer du code",
    draft: {
      name: "expliquer-du-code",
      description: "Utilise ce skill quand on demande d'expliquer comment fonctionne un morceau de code ou un fichier.",
      argument_hint: "[le fichier ou la fonction]",
      body: "# Explication de code\n\nÀ expliquer : $ARGUMENTS\n\n1. Lis le code concerné (et ce qu'il appelle) avant de répondre.\n2. Explique d'abord à quoi il sert, en une phrase, puis comment il le fait, étape par étape.\n3. Signale les pièges ou les cas limites que tu remarques.\n\nAdapte le niveau de détail : un débutant a besoin d'images, un expert veut aller vite.\n",
    },
  },
];

/** Consigne donnée au modèle pour rédiger un skill à partir d'une idée. */
export const SKILL_WRITER_PROMPT = `Tu aides à écrire des « skills » pour un assistant IA : de courtes procédures que l'assistant suit pour une tâche précise.
À partir de l'idée de l'utilisateur, écris le skill en français.
Réponds uniquement avec un objet JSON valide, sans texte autour, de cette forme :
{"name":"...","description":"...","argument_hint":"...","body":"..."}
- "name" : 2 à 4 mots en minuscules séparés par des tirets (ex. relire-un-texte).
- "description" : une ou deux phrases qui disent QUAND utiliser le skill, commençant par « Utilise ce skill quand ». C'est ce qui permet à l'assistant de le choisir.
- "argument_hint" : ce qu'on tape après /nom entre crochets (ex. [le texte à relire]), ou "" s'il n'y a rien à fournir.
- "body" : les instructions en Markdown : un titre, des étapes numérotées courtes et concrètes, le format de la réponse. Écris $ARGUMENTS à l'endroit où mettre ce que l'utilisateur fournit. Pas de blabla : l'assistant doit pouvoir suivre les étapes sans deviner.`;

/** Lit la réponse du modèle : un objet JSON, même s'il est entouré de texte ou de ```. */
export function parseSkillReply(text: string): Partial<SkillDraft> | null {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    const o = JSON.parse(text.slice(start, end + 1));
    if (typeof o !== "object" || !o) return null;
    const str = (v: unknown) => (typeof v === "string" ? v : "");
    const draft = { name: str(o.name), description: str(o.description), argument_hint: str(o.argument_hint), body: str(o.body) };
    return draft.description || draft.body ? draft : null;
  } catch {
    return null;
  }
}

/** Nom de commande : minuscules, chiffres et tirets (même règle que côté Rust). */
export function slugify(name: string): string {
  let out = "";
  for (const raw of name.trim().toLowerCase()) {
    const c = /[\p{L}\p{N}]/u.test(raw) ? raw : "-";
    if (c === "-" && (!out || out.endsWith("-"))) continue;
    out += c;
  }
  return out.replace(/-+$/, "");
}
