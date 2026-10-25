/** Progression de l'utilisateur : points d'expérience, niveaux, série de jours et badges. Logique pure, sans interface. */

export type Event =
  | { kind: "message" }
  | { kind: "answer" }
  | { kind: "code" }
  | { kind: "project" }
  | { kind: "task" }
  | { kind: "memory" }
  | { kind: "artifact" }
  | { kind: "model"; name: string }
  | { kind: "backup" }
  | { kind: "tutorial" };

export type Progress = {
  xp: number;
  /** Dernier jour d'activité (AAAA-MM-JJ, heure locale), série en cours et meilleure série. */
  day: string;
  streak: number;
  best: number;
  counts: { messages: number; answers: number; code: number; projects: number; tasks: number; memories: number; artifacts: number; backups: number };
  models: string[];
  /** Badge → date d'obtention (AAAA-MM-JJ). */
  badges: Record<string, string>;
  night: boolean;
  early: boolean;
};

export const EMPTY: Progress = {
  xp: 0, day: "", streak: 0, best: 0,
  counts: { messages: 0, answers: 0, code: 0, projects: 0, tasks: 0, memories: 0, artifacts: 0, backups: 0 },
  models: [], badges: {}, night: false, early: false,
};

export type Badge = { id: string; label: string; hint: string; icon: string; test: (p: Progress, level: number) => boolean };

export const BADGES: Badge[] = [
  { id: "first", label: "Premier mot", hint: "Envoie ton premier message.", icon: "pencil", test: (p) => p.counts.messages >= 1 },
  { id: "chatty", label: "Bavard", hint: "Envoie 50 messages.", icon: "pencil", test: (p) => p.counts.messages >= 50 },
  { id: "talker", label: "Moulin à paroles", hint: "Envoie 500 messages.", icon: "pencil", test: (p) => p.counts.messages >= 500 },
  { id: "coder", label: "Les mains dans le code", hint: "Fais modifier des fichiers par l'agent.", icon: "file", test: (p) => p.counts.code >= 1 },
  { id: "builder", label: "Bâtisseur", hint: "Mène 10 tâches de code à bien.", icon: "file", test: (p) => p.counts.code >= 10 },
  { id: "organised", label: "Organisé", hint: "Crée ton premier projet.", icon: "folder", test: (p) => p.counts.projects >= 1 },
  { id: "chief", label: "Chef de projets", hint: "Crée 3 projets.", icon: "folder", test: (p) => p.counts.projects >= 3 },
  { id: "memory", label: "Bonne mémoire", hint: "Ajoute un souvenir à la mémoire.", icon: "book", test: (p) => p.counts.memories >= 1 },
  { id: "planner", label: "Chef d'orchestre", hint: "Planifie une tâche automatique.", icon: "clock", test: (p) => p.counts.tasks >= 1 },
  { id: "creator", label: "Créateur", hint: "Garde un artéfact dans ta bibliothèque.", icon: "star", test: (p) => p.counts.artifacts >= 1 },
  { id: "explorer", label: "Explorateur", hint: "Essaie 3 modèles différents.", icon: "globe", test: (p) => p.models.length >= 3 },
  { id: "safe", label: "Prévoyant", hint: "Crée une sauvegarde de tes données.", icon: "shield", test: (p) => p.counts.backups >= 1 },
  { id: "streak3", label: "Régulier", hint: "3 jours de suite.", icon: "flame", test: (p) => p.best >= 3 },
  { id: "streak7", label: "Inséparable", hint: "7 jours de suite.", icon: "flame", test: (p) => p.best >= 7 },
  { id: "streak30", label: "Fidèle", hint: "30 jours de suite.", icon: "flame", test: (p) => p.best >= 30 },
  { id: "night", label: "Oiseau de nuit", hint: "Travaille entre minuit et 5 h.", icon: "clock", test: (p) => p.night },
  { id: "early", label: "Lève-tôt", hint: "Travaille entre 5 h et 7 h.", icon: "clock", test: (p) => p.early },
  { id: "level5", label: "Lama des cimes", hint: "Atteins le niveau 5.", icon: "trophy", test: (_p, level) => level >= 5 },
];

const XP: Record<Event["kind"], number> = { message: 5, answer: 3, code: 15, project: 20, task: 20, memory: 10, artifact: 10, model: 15, backup: 20, tutorial: 25 };

export const TITLES = ["Cria", "Jeune lama", "Lama curieux", "Lama voyageur", "Lama des cimes", "Lama sage", "Gardien des Andes", "Lama d'or", "Lama légendaire"];

/** Points nécessaires pour atteindre un niveau : 0, 60, 180, 360, 600, 900… */
export const threshold = (level: number) => 30 * level * (level - 1);

export function levelOf(xp: number): number {
  let l = 1;
  while (xp >= threshold(l + 1)) l++;
  return l;
}

export const titleOf = (level: number) => TITLES[Math.min(level, TITLES.length) - 1];

/** Avancement dans le niveau en cours : points gagnés, points à gagner, et part (0 à 1). */
export function stepOf(xp: number): { level: number; into: number; need: number; ratio: number } {
  const level = levelOf(xp);
  const from = threshold(level);
  const need = threshold(level + 1) - from;
  return { level, into: xp - from, need, ratio: (xp - from) / need };
}

/** Date locale AAAA-MM-JJ (jamais en UTC : la série suit les jours de l'utilisateur). */
export function dayKey(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

const daysBetween = (a: string, b: string) => Math.round((new Date(b + "T12:00:00").getTime() - new Date(a + "T12:00:00").getTime()) / 86_400_000);

export type Result = { state: Progress; gained: number; levelUp: number | null; unlocked: Badge[]; streakUp: boolean };

/** Applique un événement : renvoie le nouvel état, les points gagnés, un éventuel nouveau niveau et les badges obtenus. */
export function apply(prev: Progress, ev: Event, now: Date = new Date()): Result {
  const p: Progress = JSON.parse(JSON.stringify(prev));
  const levelBefore = levelOf(p.xp);
  let gained = 0;
  let streakUp = false;

  // Série : un jour d'activité de plus (hier → +1, plus ancien → on repart de 1) et un petit bonus la première fois du jour.
  const today = dayKey(now);
  if (p.day !== today) {
    p.streak = p.day && daysBetween(p.day, today) === 1 ? p.streak + 1 : 1;
    p.best = Math.max(p.best, p.streak);
    p.day = today;
    streakUp = p.streak > 1;
    gained += Math.min(5 * p.streak, 50);
  }

  let first = true;
  switch (ev.kind) {
    case "message": {
      p.counts.messages++;
      const h = now.getHours();
      if (h < 5) p.night = true;
      else if (h < 7) p.early = true;
      break;
    }
    case "answer": p.counts.answers++; break;
    case "code": p.counts.code++; break;
    case "project": p.counts.projects++; break;
    case "task": p.counts.tasks++; break;
    case "memory": p.counts.memories++; break;
    case "artifact": p.counts.artifacts++; break;
    case "backup": p.counts.backups++; break;
    case "model":
      first = !!ev.name && !p.models.includes(ev.name);
      if (first) p.models.push(ev.name);
      break;
    case "tutorial": break;
  }
  if (first) gained += XP[ev.kind];
  p.xp += gained;

  const level = levelOf(p.xp);
  const date = today;
  const unlocked: Badge[] = [];
  // Deux passes : un niveau gagné ici peut débloquer un badge de niveau.
  for (const b of BADGES) if (!p.badges[b.id] && b.test(p, level)) { p.badges[b.id] = date; unlocked.push(b); }
  return { state: p, gained, levelUp: level > levelBefore ? level : null, unlocked, streakUp };
}
