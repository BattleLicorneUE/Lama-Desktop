/** Questions de présentation : ce que le modèle doit savoir de l'utilisateur. Les réponses deviennent des souvenirs (mémoire du profil). */
export type Answers = {
  name: string;
  usages: string[];
  context: string;
  field: string;
  level: string;
  tools: string;
  length: string;
  tone: string;
  address: string;
  language: string;
  note: string;
};

export const EMPTY: Answers = { name: "", usages: [], context: "", field: "", level: "", tools: "", length: "", tone: "", address: "", language: "", note: "" };

export const USAGES = [
  { id: "chat", label: "Discuter, poser des questions" },
  { id: "write", label: "Écrire et reformuler" },
  { id: "code", label: "Programmer" },
  { id: "learn", label: "Apprendre, réviser" },
  { id: "research", label: "Rechercher, analyser" },
  { id: "translate", label: "Traduire" },
  { id: "work", label: "Documents et e-mails pro" },
  { id: "creative", label: "Idées, création" },
  { id: "data", label: "Données, tableurs" },
];

export const CONTEXTS = ["Travail", "Études", "Loisirs", "Un peu de tout"];
export const LEVELS = ["Débutant", "Intermédiaire", "Expert"];
export const LENGTHS = ["Courtes et directes", "Équilibrées", "Détaillées"];
export const TONES = ["Décontracté", "Neutre", "Formel"];
export const ADDRESSES = ["Tutoiement", "Vouvoiement"];
export const LANGUAGES = ["Français", "English", "La langue de ma question"];

/** Préfixe commun des souvenirs créés ici : permet de les remplacer sans toucher aux autres. */
export const PREFIX = "Profil — ";

const label = (id: string) => USAGES.find((u) => u.id === id)?.label.toLowerCase() ?? id;

/** Phrases enregistrées en mémoire (une par sujet, vides ignorées). */
export function toFacts(a: Answers): string[] {
  const lines: string[] = [];
  const add = (s: string) => s.trim() && lines.push(PREFIX + s.trim());
  if (a.name.trim()) add(`S'appelle ${a.name.trim()} (l'appeler ainsi).`);
  if (a.usages.length) add(`Utilise l'assistant surtout pour : ${a.usages.map(label).join(", ")}.`);
  const ctx = [a.context && `contexte ${a.context.toLowerCase()}`, a.field.trim() && `métier ou domaine : ${a.field.trim()}`].filter(Boolean).join(" ; ");
  if (ctx) add(`${ctx[0].toUpperCase()}${ctx.slice(1)}.`);
  const tech = [a.level && `niveau technique ${a.level.toLowerCase()}`, a.tools.trim() && `langages et outils : ${a.tools.trim()}`].filter(Boolean).join(" ; ");
  if (tech) add(`${tech[0].toUpperCase()}${tech.slice(1)}.`);
  const style = [
    a.length && `réponses ${a.length.toLowerCase()}`,
    a.tone && `ton ${a.tone.toLowerCase()}`,
    a.address && a.address.toLowerCase(),
    a.language && `langue : ${a.language}`,
  ].filter(Boolean).join(" ; ");
  if (style) add(`Préférences de réponse : ${style}.`);
  if (a.note.trim()) add(`À savoir : ${a.note.trim()}`);
  return lines;
}

/** Les étapes affichées : la partie technique n'est proposée qu'à ceux qui programment. */
export function steps(a: Answers): string[] {
  return ["name", "usages", "context", ...(a.usages.includes("code") ? ["tech"] : []), "style", "note", "recap"];
}
