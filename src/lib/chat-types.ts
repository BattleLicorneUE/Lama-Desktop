/** Types et petites fonctions partagés par le magasin de conversations (chat.svelte.ts) et les composants. */

import { load, save } from "$lib/chat-storage";
import { EMPTY as EMPTY_PROGRESS, type Progress } from "$lib/progress";
import type { ScheduledTask } from "$lib/schedule";

export type Mode = "chat" | "code";
export type Theme = "system" | "light" | "dark";
export type ToolState = "awaiting" | "running" | "done" | "error" | "denied" | "undone";
export type ToolEvent = {
  call_id: string;
  name: string;
  args: Record<string, unknown>;
  state: ToolState;
  output?: string | null;
  diff?: string | null;
  /** Sous-agent qui a lancé l'outil (délégation), absent pour l'agent principal. */
  agent?: string | null;
};
export type Task = { content: string; status: "pending" | "in_progress" | "completed" };
export type Attachment = {
  name: string; kind: "image" | "text"; data: string;
  /** Image générée : fichier enregistré sur le PC. */ path?: string;
  /** Poids estimé d'un fichier texte, et façon de l'envoyer : en entier, ou en extraits que le modèle complète à la demande. */
  tokens?: number; mode?: "full" | "excerpt";
};
export type Msg = {
  role: "user" | "assistant" | "system";
  content: string;
  thinking?: string | null;
  tools?: ToolEvent[];
  tasks?: Task[];
  attachments?: Attachment[];
  /** Commentaires écrits par le modèle entre deux étapes (avant quel outil ils ont été dits) : rangés avec le détail des actions. */
  notes?: { text: string; at: number }[];
  /** Nombre d'outils au début de l'étape en cours (sert à placer le prochain commentaire ; non enregistré). */
  stepStart?: number;
  /** Réponse coupée avant la fin : on peut reprendre la tâche. */
  interrupted?: boolean;
  /** Réponse interrompue déjà reprise : non renvoyée au modèle (la note de reprise la résume). */
  skipWire?: boolean;
};
export type Model = { name: string; size: number; remote_host?: string | null; /** Nom affiché si différent de `name` (modèles locaux). */ label?: string; local?: boolean };

/** Un modèle installé sur ce PC (moteur local) ; dans la liste des modèles il s'appelle `local:<id>`. */
export type LocalInstalled = {
  id: string;
  name: string;
  size_bytes: number;
  tools: boolean;
  vision: boolean;
  license: string;
  /** Contexte utilisé (jetons), choix de l'utilisateur (0 = automatique), valeur automatique et maximum du modèle. */
  ctx: number;
  ctx_setting: number;
  auto_ctx: number;
  max_ctx: number;
  options: { ctx: number; mem_mb: number }[];
};
export type LocalProgress = { id: string; phase: string; file: string; done: number; total: number };
export const LOCAL_PREFIX = "local:";

/** 5 027 783 488 -> « 4,7 Go » ; en dessous d'un gigaoctet, en Mo. */
export function fmtBytes(b: number): string {
  return b >= 1024 ** 3 ? `${(b / 1024 ** 3).toFixed(1).replace(".", ",")} Go` : `${Math.max(1, Math.round(b / 1024 ** 2))} Mo`;
}


/** Modèle proposé tant qu'on n'en a pas choisi un autre : « beam » (gratuit, 1M de contexte, outils) s'il est au catalogue. */
export const DEFAULT_MODEL = "beam";
export function preferredModel(models: Model[]): Model | undefined {
  return models.find((m) => m.name === DEFAULT_MODEL) ?? models.find((m) => m.name.startsWith(DEFAULT_MODEL + ":")) ?? models[0];
}

/** Les versions précédentes préfixaient les modèles cloud par `cloud/` : on l'enlève des anciennes conversations. */
export const cleanModel = (name: string) => name.replace(/^cloud\//, "");
export type Conv = {
  id: string; title: string; model: string; updated_at: number; kind: Mode;
  workspace: string | null; project_id: string | null;
  pinned?: boolean; snippet?: string | null;
};
export type SavedArtifact = { id: string; title: string; lang: string; kind: string; code: string; conv_id: string | null; created_at: number; updated_at: number; versions: number };
export type Project = { id: string; name: string; instructions: string; kind: "chat" | "code"; workspace: string | null; pinned: boolean; updated_at: number; chats: number };
export type Skill = {
  name: string; description: string; dir: string; scope: "app" | "project"; enabled: boolean;
  argument_hint?: string | null; model_invocable?: boolean; user_invocable?: boolean;
};
export type AgentProfile = {
  name: string; description: string; prompt: string; model: string | null; tools: string[] | null;
  permission: string | null; skills: string[] | null; scope: "builtin" | "app" | "project";
};
/** Jetons consommés par ce profil : aujourd'hui et au total. */
export type Usage = {
  day: string; dayIn: number; dayOut: number; totalIn: number; totalOut: number;
  /** Historique par jour (AAAA-MM-JJ) et par modèle : jetons envoyés (i) et générés (o). */
  days: Record<string, { i: number; o: number }>;
  models: Record<string, { i: number; o: number }>;
};
export const today = () => new Date().toISOString().slice(0, 10);

export function loadProgress(): Progress {
  const fresh = (): Progress => JSON.parse(JSON.stringify(EMPTY_PROGRESS));
  try {
    const o = JSON.parse(load("progress", "{}"));
    return { ...fresh(), ...o, counts: { ...EMPTY_PROGRESS.counts, ...o.counts } };
  } catch {
    return fresh();
  }
}

export type Celebration = { id: number; kind: "badge" | "level"; title: string; text: string; icon: string };

export function loadUsage(): Usage {
  const empty: Usage = { day: today(), dayIn: 0, dayOut: 0, totalIn: 0, totalOut: 0, days: {}, models: {} };
  try {
    return { ...empty, ...JSON.parse(load("usage", "{}")) };
  } catch {
    return empty;
  }
}

/** Valeur du paramètre `think` d'Ollama : booléen, ou niveau pour gpt-oss qui ne se désactive pas. */
export function thinkValue(model: string, on: boolean, effort: string): boolean | string {
  if (!/^gpt-oss/i.test(model)) return on;
  if (!on) return "low";
  return effort === "low" ? "low" : effort === "medium" ? "medium" : "high";
}

export type Memory = { id: number; text: string };
export type ProjectDoc = { id: string; name: string; content: string };
export type DocDraft = { id?: string; name: string; content: string };

/** Contexte supposé quand Ollama n'indique pas la limite du modèle. */
export const DEFAULT_CTX = 32768;
/** Plafond de sécurité (1 million de jetons). */
export const MAX_CTX = 1048576;
/** Part du contexte à partir de laquelle la conversation est compactée toute seule. */
export const COMPACT_AT = 0.7;

export function formatTokens(n: number): string {
  if (n >= 1048576) return `${+(n / 1048576).toFixed(1)} M`;
  if (n >= 1024) return `${Math.round(n / 1024)} k`;
  return String(n);
}

/** Limite de contexte d'un modèle, lue dans son `model_info` (`<architecture>.context_length`). */
export function parseCtxInfo(modelInfo: Record<string, unknown> | undefined): CtxInfo {
  const mi = modelInfo ?? {};
  const v = Number(mi[`${String(mi["general.architecture"] ?? "")}.context_length`]);
  return { maxCtx: Number.isFinite(v) && v > 0 ? v : null };
}
export type CtxInfo = { maxCtx: number | null };

/** Estimation grossière du nombre de jetons d'un texte (~3,5 caractères par jeton). */
export const estimateTokens = (text: string) => Math.ceil(text.length / 3.5);

/** Capacités annoncées par Ollama (`/api/show`), avec leur libellé ; « completion » est implicite. */
export const CAP_LABELS: Record<string, string> = {
  vision: "Vision",
  tools: "Outils",
  thinking: "Réflexion",
  audio: "Audio",
  embedding: "Embedding",
  insert: "Insertion",
};

/** Commandes slash intégrées (les skills s'y ajoutent). */
/** Consigne de la recherche approfondie : plusieurs agents « chercheur » en parallèle, puis un rapport sourcé. */
export const RESEARCH_PROMPT = `Mode RECHERCHE APPROFONDIE. Procède ainsi :
1) Reformule la question et découpe-la en 3 à 6 sous-questions indépendantes (définitions et contexte, chiffres récents, points de vue ou acteurs, comparaisons, controverses, actualité).
2) Confie chaque sous-question à l'agent « chercheur » avec l'outil delegate, TOUTES dans le même tour pour qu'elles s'exécutent en parallèle. Dans chaque tâche, demande des faits précis et datés avec l'URL de la source.
3) Lis les résumés reçus, repère les contradictions et les lacunes. Si une lacune importante demeure, lance un seul second tour de chercheurs ciblés.
4) Rédige alors un rapport en français : titre, résumé en cinq lignes, sections avec sous-titres, chiffres datés, désaccords entre sources, limites et incertitudes, puis la liste des sources sous forme de liens.
N'invente rien : chaque affirmation importante vient d'une source citée. Ce que tu n'as pas trouvé, dis-le.`;

export const BUILTIN_COMMANDS = [
  { name: "recherche", hint: "Recherche approfondie : /recherche ta question (agents en parallèle, rapport avec sources)" },
  { name: "remember", hint: "Mémoriser un fait : /remember Je préfère les réponses courtes" },
  { name: "compact", hint: "Résumer la conversation pour libérer du contexte" },
  { name: "memoire", hint: "Compacter cette conversation et la garder en mémoire (envoyée au modèle dans tes futures conversations)" },
  { name: "clear", hint: "Nouvelle conversation" },
];
export type Stored = { role: Msg["role"]; content: string; thinking?: string | null; extra?: string | null };

export const isWrite = (name: string) => ["write_file", "edit_file", "make_dir", "move_path", "delete_path"].includes(name);

/** Traduit une erreur technique en piste concrète pour l'utilisateur. */
export function explain(error: string): string {
  const e = error.toLowerCase();
  if (e.includes("injoignable") || e.includes("connection") || e.includes("refused"))
    return "Ollama ne répond plus. Vérifie qu'il est démarré, puis réessaie.";
  if (e.includes("failed to load") || e.includes("memory") || e.includes("out of"))
    return "Le modèle n'a pas pu se charger, probablement faute de mémoire. Essaie un modèle plus petit ou réduis le contexte dans les Réglages.";
  if (e.includes("429") || e.includes("limite d'usage") || e.includes("too many"))
    return "Tu as atteint la limite d'usage de ton compte Ollama Cloud (l'app a déjà réessayé plusieurs fois). Attends un peu, ou choisis un modèle plus léger ; les sous-agents en parallèle consomment plus vite.";
  if (e.includes("not found"))
    return "Modèle introuvable sur Ollama Cloud : choisis-en un autre dans le gestionnaire de modèles (Ctrl+M).";
  if (e.includes("does not support tools") || e.includes("tools"))
    return "Ce modèle ne gère pas les outils : choisis-en un qui affiche « tools » dans le gestionnaire de modèles.";
  return "";
}

/** Message tel qu'envoyé au modèle : fichiers texte en bloc, images à part. */
export function fromStored(s: Stored): Msg {
  const msg: Msg = { role: s.role, content: s.content, thinking: s.thinking };
  if (s.extra) {
    try {
      const x = JSON.parse(s.extra);
      if (x.tools) msg.tools = x.tools;
      if (x.tasks) msg.tasks = x.tasks;
      if (x.notes) msg.notes = x.notes;
      if (x.attachments) msg.attachments = x.attachments;
      if (x.interrupted) msg.interrupted = true;
      if (x.skipWire) msg.skipWire = true;
    } catch {}
  }
  return msg;
}
