import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { tick } from "svelte";
import { MODE_IDS, PLAN_HINT, codeRequest } from "$lib/code-options";
import { CREATIONS, lastOpenable, type CreationKind } from "$lib/artifacts";
import { attachmentsForTool, fileToBase64, prepare, shrinkImage, splitProjectDocs, wireHistory } from "$lib/attachments";
import { resumeNote } from "$lib/resume";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { REVIEW_PROMPT, annotate, parseDiff, parseReview, type ReviewResult } from "$lib/review";
import { fileSlug, toDocMarkdown, toJson, toMarkdown } from "$lib/export";
import { MEMORY_PROMPT, memoryLine, parseMemory } from "$lib/memoire";
import { notify } from "$lib/notify";
import { nextRun, type ScheduledTask } from "$lib/schedule";
import { EMPTY as EMPTY_PROGRESS, apply as applyProgress, titleOf, type Event as ProgressEvent, type Progress } from "$lib/progress";
import { PREFIX as FACT_PREFIX, type Answers } from "$lib/onboarding";

const PREFIX = "ollama-desktop.";
let namespace = "";
let inheritsLegacy = false;

/** Réglages de l'interface propres à un profil (clé API, thème, modèle…). Le profil issu de l'ancienne version reprend les anciens réglages. */
export function setProfile(id: string, legacy: boolean) {
  namespace = `p.${id}.`;
  inheritsLegacy = legacy;
}

/** Clé API éventuellement gardée en clair dans l'interface (ancienne version, ou repli si le coffre Windows est indisponible). */
export const peekStoredKey = () => load("webKey", "");

/** Efface cette clé locale une fois qu'elle est en sécurité dans le coffre Windows. */
export function clearStoredKey() {
  try {
    localStorage.removeItem(PREFIX + namespace + "webKey");
    if (inheritsLegacy) localStorage.removeItem(PREFIX + "webKey");
  } catch {}
}

function load(key: string, fallback: string): string {
  try {
    return localStorage.getItem(PREFIX + namespace + key) ?? (inheritsLegacy ? localStorage.getItem(PREFIX + key) : null) ?? fallback;
  } catch {
    return fallback;
  }
}

function save(key: string, value: string) {
  try {
    localStorage.setItem(PREFIX + namespace + key, value);
  } catch {}
}

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
const today = () => new Date().toISOString().slice(0, 10);

function loadProgress(): Progress {
  const fresh = (): Progress => JSON.parse(JSON.stringify(EMPTY_PROGRESS));
  try {
    const o = JSON.parse(load("progress", "{}"));
    return { ...fresh(), ...o, counts: { ...EMPTY_PROGRESS.counts, ...o.counts } };
  } catch {
    return fresh();
  }
}

export type Celebration = { id: number; kind: "badge" | "level"; title: string; text: string; icon: string };

function loadUsage(): Usage {
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
type Stored = { role: Msg["role"]; content: string; thinking?: string | null; extra?: string | null };

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
function fromStored(s: Stored): Msg {
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

/**
 * Une conversation avec son état de travail. Celle qu'on regarde est `fg` ; quand on en ouvre une autre pendant
 * qu'elle travaille, elle passe en arrière-plan (`parked`) et continue sans être interrompue.
 */
class Session {
  activeId = $state<string | null>(null);
  messages = $state<Msg[]>([]);
  busy = $state(false);
  error = $state("");
  promptTokens = $state(0);
  /** Précisions envoyées pendant la tâche, pas encore lues par le modèle. */
  queued = $state<string[]>([]);
  currentId = "";
  agentRun = false;
  model = "";
  /** Texte reçu pas encore affiché : le rendu Markdown de toute la réponse ne se refait qu'une dizaine de fois par seconde. */
  chunkBuf = { content: "", thinking: "" };
  chunkTimer: ReturnType<typeof setTimeout> | null = null;
  saveTimer: ReturnType<typeof setTimeout> | null = null;
  /** Type de création choisi dans la page Artéfacts : classe l'artefact produit par cette conversation. */
  artifactKind = "";
}

/** État partagé de l'app : modes Chat / Code, conversations, streaming, agent. */
export class ChatStore {
  constructor(apiKey = "") {
    this.webKey = apiKey;
  }

  mode = $state<Mode>("chat");
  /** Modèles d'Ollama Cloud accessibles avec la clé API. */
  models = $state<Model[]>([]);
  cloudModels = $state<Model[]>([]);
  localInstalled = $state<LocalInstalled[]>([]);
  /** Installation en cours (une seule à la fois) et dernière erreur d'installation. */
  localProgress = $state<LocalProgress | null>(null);
  localError = $state("");
  /** État de la connexion à l'API cloud : « ok » seulement après un vrai test authentifié. */
  cloudStatus = $state<"none" | "checking" | "listed" | "ok" | "error">("none");
  cloudError = $state("");
  model = $state("");
  /** Conversation affichée, et conversations qui travaillent en arrière-plan. */
  fg = $state(new Session());
  parked = $state<Session[]>([]);
  get messages() { return this.fg.messages; }
  set messages(v: Msg[]) { this.fg.messages = v; }
  get activeId() { return this.fg.activeId; }
  set activeId(v: string | null) { this.fg.activeId = v; }
  get busy() { return this.fg.busy; }
  set busy(v: boolean) { this.fg.busy = v; }
  get error() { return this.fg.error; }
  set error(v: string) { this.fg.error = v; }
  get promptTokens() { return this.fg.promptTokens; }
  set promptTokens(v: number) { this.fg.promptTokens = v; }
  get queued() { return this.fg.queued; }
  set queued(v: string[]) { this.fg.queued = v; }
  private get currentId() { return this.fg.currentId; }
  private set currentId(v: string) { this.fg.currentId = v; }
  convs = $state<Conv[]>([]);
  workspace = $state("");
  projects = $state<Project[]>([]);
  /** Page affichée : le chat, la liste des projets ou la page d'un projet. */
  view = $state<"chat" | "projects" | "project" | "artifacts" | "tasks">("chat");
  /** Bibliothèque d'artefacts de la page Artéfacts. */
  artifacts = $state<SavedArtifact[]>([]);
  /** Type de création en préparation (Documents, Diapositives…) : consigne ajoutée au prochain envoi. */
  creating = $state<CreationKind | null>(null);
  /** Fenêtre de création (« new ») ou de modification d'un projet. */
  projectDialog = $state<Project | "new" | null>(null);
  /** Projet sélectionné : filtre la liste et reçoit les nouveaux chats. */
  activeProject = $state<string | null>(null);
  /** Projet de la conversation en cours (instructions appliquées, projet des nouveaux messages). */
  chatProject = $state<string | null>(null);
  search = $state("");
  pending = $state<Attachment[]>([]);
  /** Aperçu d'artefact (bloc html/svg) ouvert à droite. */
  preview = $state<{ code: string; lang: string } | null>(null);
  /** Capacités des modèles (`tools`, `vision`…), chargées à la demande. */
  caps = $state<Record<string, string[]>>({});
  ctxInfo = $state<Record<string, CtxInfo>>({});
  /** Modèles dont Ollama répond « 410 » (retirés du service cloud). */
  unavailable = $state<Record<string, boolean>>({});
  /** Modifications de fichiers de l'agent annulables pendant cette session. */
  sessionChanges = $state(0);
  skills = $state<Skill[]>([]);
  memories = $state<Memory[]>([]);
  /** Agents disponibles : l'agent principal décide seul s'il leur confie une tâche (outil delegate). */
  agents = $state<AgentProfile[]>([]);
  usage = $state<Usage>(loadUsage());
  /** Présentation de l'utilisateur (questions de bienvenue) faite ou passée, et fenêtre rouverte depuis les Réglages. */
  onboarded = $state(load("onboarded", "0") === "1");
  onboardingOpen = $state(false);
  /** Tutoriel de bienvenue (compte, clé, modèle) : déjà vu — ou inutile pour qui s'est déjà présenté —, et fenêtre rouverte depuis les Réglages. */
  tutorialDone = $state(load("tutorial", "0") === "1" || load("onboarded", "0") === "1");
  tutorialOpen = $state(false);
  /** Change à chaque enregistrement de la présentation : les écrans qui l'affichent se rechargent. */
  onboardVersion = $state(0);
  /** Nom du profil ouvert (renseigné à l'ouverture), repris dans les notes enregistrées. */
  profileName = "";
  savingMemory = $state(false);
  /** Notification Windows quand une réponse est prête ou qu'une autorisation est demandée, fenêtre en arrière-plan. */
  notifications = $state(load("notify", "1") === "1");
  /** Réflexion des modèles qui la gèrent : « auto » laisse le modèle décider. */
  think = $state<"auto" | "on" | "off">(load("think", "auto") as "auto" | "on" | "off");
  /** Mode choisi avant de passer en Plan, pour y revenir à l'approbation du plan. */
  /** Message d'information passager. */
  compacting = $state(false);
  notice = $state("");
  scroller: HTMLElement | undefined;
  /** Texte à placer dans la zone de saisie (suggestions de l'écran d'accueil) : le composeur le prend puis le vide. */
  draft = $state("");

  /** Réglages mémorisés entre les lancements. */
  /** Mode d'autorisation (menu sous la zone de saisie) : auto, manual, accept-edits, plan ou bypass. Valable en Chat comme en Code. */
  permission = $state(MODE_IDS.includes(load("permission", "auto")) ? load("permission", "auto") : "auto");
  private beforePlan = load("beforePlan", "auto");
  effort = $state(load("effort", "medium"));
  theme = $state<Theme>(load("theme", "system") as Theme);
  /** Affichage de la conversation : taille du texte, largeur des colonnes, animations. */
  textSize = $state(load("textSize", "medium") as "small" | "medium" | "large");
  convWidth = $state(load("convWidth", "narrow") as "narrow" | "medium" | "wide");
  motion = $state(load("motion", "system") as "system" | "reduced");
  font = $state(load("font", "serif") as "serif" | "sans" | "mono" | "readable");
  /** Progression : points, niveau, série et badges (réglable : certains n'en veulent pas). */
  progress = $state<Progress>(loadProgress());
  gamify = $state(load("gamify", "1") === "1");
  celebrations = $state<Celebration[]>([]);
  private celebId = 0;
  /** Derniers points gagnés (pour l'animation « +N » de la puce de niveau). */
  xpGain = $state({ n: 0, at: 0 });
  /** Réglages à ouvrir sur une page précise (la progression, depuis la puce ou un badge). */
  settingsTab = $state<string | null>(null);
  settingsWanted = $state(false);
  /** Tâches planifiées du profil. */
  tasks = $state<ScheduledTask[]>([]);
  /** Mémoire du projet ouvert (page projet). */
  projectMemories = $state<Memory[]>([]);
  /** Recherche web (outils web_search / web_fetch) activée pour l'agent. */
  webSearch = $state(load("web", "1") === "1");
  /** Clé API d'Ollama Cloud (modèles cloud). Nom historique : `webKey`. */
  webKey = $state("");
  /** Modèle utilisé en mode Plan (vide = le modèle courant). */
  planModel = $state(load("planModel", ""));
  /** Étiquette de cette fenêtre (« main », puis « win-… » pour les fenêtres supplémentaires). */
  windowLabel = getCurrentWebviewWindow().label;
  /** Revue de code : résultat de la dernière relecture des changements du projet. */
  review = $state<ReviewResult | null>(null);
  reviewing = $state(false);
  reviewError = $state("");
  /** Relire automatiquement les changements à la fin d'une tâche de l'agent. */
  autoReview = $state(load("autoReview", "0") === "1");
  private changesBeforeRun = 0;
  /** Prochain envoi en recherche approfondie (se désactive après un envoi). */
  researchNext = $state(false);
  /** Volets repliables ; par défaut fermés sur les fenêtres étroites. */
  sidebarOpen = $state(load("sidebar", window.innerWidth < 800 ? "0" : "1") === "1");
  panelOpen = $state(load("panel", window.innerWidth < 1150 ? "0" : "1") === "1");

  private queueChunk(s: Session, content: string, thinking: string) {
    s.chunkBuf.content += content;
    s.chunkBuf.thinking += thinking;
    s.chunkTimer ??= setTimeout(() => this.flushChunks(s), 80);
  }

  private flushChunks(s: Session = this.fg) {
    if (s.chunkTimer) clearTimeout(s.chunkTimer);
    s.chunkTimer = null;
    const { content, thinking } = s.chunkBuf;
    s.chunkBuf = { content: "", thinking: "" };
    const last = s.messages[s.messages.length - 1];
    if (!last || (!content && !thinking)) return;
    if (content) last.content += content;
    if (thinking) last.thinking = (last.thinking ?? "") + thinking;
    if (s === this.fg) void this.scrollDown();
  }

  /** Abandonne le texte en attente. */
  private dropChunks(s: Session = this.fg) {
    if (s.chunkTimer) clearTimeout(s.chunkTimer);
    s.chunkTimer = null;
    s.chunkBuf = { content: "", thinking: "" };
  }

  /** La session dont la requête porte cet identifiant (affichée ou en arrière-plan). */
  private sess(id: string): Session | undefined {
    return this.fg.currentId === id ? this.fg : this.parked.find((p) => p.currentId === id);
  }

  /** Cette conversation travaille-t-elle en ce moment (affichée ou non) ? */
  isRunning(convId: string): boolean {
    return [this.fg, ...this.parked].some((s) => s.busy && s.activeId === convId);
  }

  get anyRunning(): boolean {
    return this.fg.busy || this.parked.length > 0;
  }

  /** Quitte la conversation affichée : si elle travaille, elle continue en arrière-plan. */
  private detach() {
    if (this.fg.busy) this.parked.push(this.fg);
    else this.dropChunks();
    this.fg = new Session();
  }

  /** Une session en arrière-plan a fini : on la retire de la liste. */
  private unpark(s: Session) {
    this.parked = this.parked.filter((p) => p !== s);
  }

  setPermission(id: string) {
    if (!MODE_IDS.includes(id)) return;
    if (id === "plan" && this.permission !== "plan") {
      this.beforePlan = this.permission;
      save("beforePlan", this.permission);
    }
    this.permission = id;
    save("permission", id);
  }

  /** Demandes d'autorisation en attente (la première s'affiche dans une fenêtre). */
  get pendingApprovals(): ToolEvent[] {
    return [this.fg, ...this.parked].flatMap((s) => s.messages.flatMap((m) => m.tools ?? [])).filter((t) => t.state === "awaiting");
  }

  setNotifications(v: boolean) {
    this.notifications = v;
    save("notify", v ? "1" : "0");
  }

  private alert(title: string, body: string) {
    if (this.notifications) void notify(title, body);
  }

  setThink(v: "auto" | "on" | "off") {
    this.think = v;
    save("think", v);
  }

  /** Mode Plan terminé : le dernier message est une réponse de l'assistant à valider. */
  get planReady(): boolean {
    const last = this.messages[this.messages.length - 1];
    return this.permission === "plan" && !this.busy && last?.role === "assistant" && !!last.content.trim();
  }

  /** Approuve le plan : revient au mode précédent et demande l'exécution. */
  async approvePlan() {
    this.setPermission(this.beforePlan === "plan" ? "auto" : this.beforePlan);
    await this.send("Le plan est approuvé. Exécute-le maintenant, étape par étape, puis vérifie le résultat.");
  }

  private addUsage(promptTokens: number, evalTokens: number, model = "") {
    const u = this.usage;
    if (u.day !== today()) Object.assign(u, { day: today(), dayIn: 0, dayOut: 0 });
    u.dayIn += promptTokens;
    u.dayOut += evalTokens;
    u.totalIn += promptTokens;
    u.totalOut += evalTokens;
    const d = (u.days[today()] ??= { i: 0, o: 0 });
    d.i += promptTokens;
    d.o += evalTokens;
    for (const k of Object.keys(u.days).sort().slice(0, -90)) delete u.days[k];
    if (model) {
      const m = (u.models[model] ??= { i: 0, o: 0 });
      m.i += promptTokens;
      m.o += evalTokens;
    }
    save("usage", JSON.stringify($state.snapshot(u)));
  }

  /** Exporte une conversation en Markdown ou en JSON (selon l'extension choisie). */
  async exportConv(c: Conv) {
    try {
      const stored = await invoke<Stored[]>("conv_messages", { id: c.id });
      const path = await saveDialog({
        title: "Exporter la conversation",
        defaultPath: `${fileSlug(c.title)}.md`,
        filters: [
          { name: "PDF", extensions: ["pdf"] },
          { name: "Word", extensions: ["docx"] },
          { name: "Excel (tableaux)", extensions: ["xlsx"] },
          { name: "CSV (tableaux)", extensions: ["csv"] },
          { name: "Page web", extensions: ["html"] },
          { name: "Markdown", extensions: ["md"] },
          { name: "JSON", extensions: ["json"] },
        ],
      });
      if (!path) return;
      const meta = { title: c.title, model: c.model, workspace: c.workspace, date: new Date() };
      const ext = path.toLowerCase().split(".").pop();
      if (ext === "json") await invoke("save_text", { path, content: toJson(stored, meta) });
      else if (ext === "md") await invoke("save_text", { path, content: toMarkdown(stored, meta) });
      else await invoke("export_document", { path, title: c.title, markdown: toDocMarkdown(stored) });
      this.flash("Conversation exportée.");
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Exporte une seule réponse en PDF, Word, page web ou Markdown. */
  async exportText(title: string, markdown: string) {
    try {
      const path = await saveDialog({
        title: "Exporter la réponse",
        defaultPath: `${fileSlug(title)}.pdf`,
        filters: [
          { name: "PDF", extensions: ["pdf"] },
          { name: "Word", extensions: ["docx"] },
          { name: "Excel (tableaux)", extensions: ["xlsx"] },
          { name: "CSV (tableaux)", extensions: ["csv"] },
          { name: "Page web", extensions: ["html"] },
          { name: "Markdown", extensions: ["md"] },
        ],
      });
      if (!path) return;
      await invoke("export_document", { path, title, markdown });
      this.flash("Document exporté.");
    } catch (e) {
      this.error = String(e);
    }
  }

  setEffort(id: string) {
    this.effort = id;
    save("effort", id);
  }

  setTheme(t: Theme) {
    this.theme = t;
    save("theme", t);
    this.applyTheme();
  }

  setWebSearch(v: boolean) {
    this.webSearch = v;
    save("web", v ? "1" : "0");
  }

  setWebKey(v: string) {
    this.webKey = v.trim();
    // Coffre Windows ; si indisponible, repli sur le stockage de l'interface (et on le dit).
    invoke("secret_set", { value: this.webKey }).catch(() => {
      save("webKey", this.webKey);
      this.flash("Coffre Windows indisponible : la clé est gardée localement.");
    });
  }

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    save("sidebar", this.sidebarOpen ? "1" : "0");
  }

  togglePanel() {
    this.panelOpen = !this.panelOpen;
    save("panel", this.panelOpen ? "1" : "0");
  }

  /** Démarre une tâche confiée à cette fenêtre (lancée depuis le tableau des agents). */
  async startTask(task: { prompt: string; workspace: string | null; mode: string }) {
    for (let i = 0; i < 60 && !this.model; i++) await new Promise((r) => setTimeout(r, 250));
    if (task.workspace && task.mode === "code") {
      await this.setMode("code");
      this.workspace = task.workspace;
    }
    await this.send(task.prompt);
  }

  /** Nouvelle fenêtre sur le même profil : conversation, dossier de travail et agent indépendants, en parallèle. */
  async openWindow() {
    try {
      await invoke("window_new");
    } catch (e) {
      this.error = String(e);
    }
  }

  setAutoReview(v: boolean) {
    this.autoReview = v;
    save("autoReview", v ? "1" : "0");
  }

  /** Revue de code : le modèle relit le diff de tout le projet (modifié et nouveau) et signale bugs et risques ligne par ligne. */
  async reviewChanges(auto = false) {
    if (this.reviewing || !this.workspace.trim()) return;
    this.reviewing = true;
    this.reviewError = "";
    try {
      const raw = await invoke<string>("git_review_diff", { workspace: this.workspace.trim() });
      const files = parseDiff(raw);
      if (!files.length) {
        this.review = null;
        if (!auto) this.reviewError = "Aucun changement à relire : le projet est identique au dernier commit.";
        return;
      }
      const model = this.models.some((m) => m.name === this.planModel) ? this.planModel : this.model;
      const budget = Math.floor(this.ctxFor(model) * 2.5);
      const out = await invoke<string>("chat_once", {
        model,
        cloudKey: this.webKey,
        options: { num_ctx: this.ctxFor(model) },
        messages: [{ role: "system", content: REVIEW_PROMPT }, { role: "user", content: annotate(files).slice(0, budget) }],
      });
      const { summary, findings } = parseReview(out, files);
      this.review = { summary, findings, at: Date.now(), files: files.length, diff: files };
      if (auto) this.flash(findings.length ? `Revue automatique : ${findings.length} remarque${findings.length > 1 ? "s" : ""} (onglet Revue).` : "Revue automatique : rien à signaler.");
    } catch (e) {
      const msg = String(e instanceof Error ? e.message : e);
      this.reviewError = /not a git repository|pas un dépôt/i.test(msg) ? "Ce dossier n'est pas un dépôt git : la revue relit les changements git (demande à l'agent de lancer git init)." : msg;
      if (auto) this.reviewError = "";
    } finally {
      this.reviewing = false;
    }
  }

  setPlanModel(name: string) {
    this.planModel = name;
    save("planModel", name);
  }

  /** Limite de contexte du modèle courant, si Ollama l'a indiquée. */
  get modelMaxCtx(): number | null {
    return this.ctxInfo[this.model]?.maxCtx ?? null;
  }

  /** Contexte utilisé pour un modèle : son maximum annoncé par Ollama, sans réglage manuel. */
  ctxFor(name: string): number {
    return Math.min(this.ctxInfo[name]?.maxCtx ?? DEFAULT_CTX, MAX_CTX);
  }

  get ctxLimit(): number {
    return this.ctxFor(this.model);
  }

  flash(msg: string) {
    this.notice = msg;
    setTimeout(() => {
      if (this.notice === msg) this.notice = "";
    }, 4000);
  }

  setDisplay(key: "textSize" | "convWidth" | "motion" | "font", value: string) {
    (this as any)[key] = value;
    save(key, value);
    this.applyTheme();
  }

  applyTheme() {
    if (this.theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = this.theme;
    const d = document.documentElement.dataset;
    d.text = this.textSize;
    d.width = this.convWidth;
    d.motion = this.motion;
    d.font = this.font;
  }

  async refresh() {
    await this.loadCloud();
  }

  /** Choix explicite du modèle : il est gardé et resservi au prochain lancement. */
  setModel(name: string) {
    this.model = name;
    save("model", name);
    if (name) this.track({ kind: "model", name });
    void this.loadCaps(name);
  }

  /** Recompose la liste affichée et garde un modèle valide sélectionné. */
  mergeModels() {
    const local: Model[] = this.localInstalled.map((m) => ({ name: LOCAL_PREFIX + m.id, size: m.size_bytes, label: `${m.name} (local)`, local: true }));
    this.models = [...this.cloudModels, ...local];
    if ((!this.model || !this.models.some((m) => m.name === this.model)) && this.models.length) {
      // Au lancement : le modèle choisi la dernière fois, s'il existe encore ; sinon beam, ou à défaut le premier de la liste.
      const saved = load("model", "");
      this.model = this.models.some((m) => m.name === saved) ? saved : preferredModel(this.models)!.name;
    }
    this.loadCaps(this.model);
  }

  get hasModels() {
    return this.models.length > 0;
  }

  /** Charge le catalogue cloud (liste publique : ne prouve pas que la clé est valide). */
  async loadCloud() {
    if (!this.webKey) {
      this.cloudModels = [];
      this.cloudStatus = "none";
      this.mergeModels();
      return;
    }
    this.cloudStatus = this.cloudStatus === "ok" ? "ok" : "checking";
    try {
      const list = await invoke<Model[]>("cloud_models", { key: this.webKey });
      this.cloudModels = list;
      if (this.cloudStatus !== "ok") this.cloudStatus = "listed";
      this.cloudError = "";
    } catch (e) {
      this.cloudModels = [];
      this.cloudStatus = "error";
      this.cloudError = String(e);
    }
    this.mergeModels();
  }

  /** Enregistre la clé, charge le catalogue puis la teste avec un tout petit message. */
  async connectCloud(key: string) {
    this.setWebKey(key);
    this.cloudStatus = "checking";
    this.cloudError = "";
    await this.loadCloud();
    if ((this.cloudStatus as string) === "error") return;
    const test = preferredModel(this.cloudModels);
    if (!test) return;
    try {
      await invoke("cloud_test", { key: this.webKey, model: test.name });
      this.cloudStatus = "ok";
    } catch (e) {
      this.cloudStatus = "error";
      this.cloudError = String(e);
    }
  }

  disconnectCloud() {
    this.setWebKey("");
    this.cloudModels = [];
    this.cloudStatus = "none";
    this.cloudError = "";
    this.mergeModels();
  }

  async loadLocal() {
    try {
      this.localInstalled = await invoke<LocalInstalled[]>("local_installed");
    } catch {
      this.localInstalled = [];
    }
    // Capacités et contexte des modèles locaux : connus d'avance, et le contexte change quand l'utilisateur le règle.
    for (const m of this.localInstalled) {
      this.caps[LOCAL_PREFIX + m.id] = ["completion", ...(m.tools ? ["tools"] : []), ...(m.vision ? ["vision"] : [])];
      this.ctxInfo[LOCAL_PREFIX + m.id] = { maxCtx: m.ctx };
    }
    this.mergeModels();
  }

  /** Règle la taille du contexte d'un modèle local (0 = automatique) ; le moteur est relancé au prochain message. */
  async setLocalCtx(id: string, ctx: number) {
    try {
      await invoke("local_set_ctx", { id, ctx });
      await this.loadLocal();
    } catch (e) {
      this.localError = String(e);
    }
  }

  async installLocal(id: string) {
    this.localError = "";
    this.localProgress = { id, phase: "préparation", file: "", done: 0, total: 0 };
    try {
      await invoke("local_install", { id });
    } catch (e) {
      this.localError = String(e);
      this.localProgress = null;
    }
  }

  cancelLocal(id: string) {
    void invoke("local_cancel", { id });
  }

  async removeLocal(id: string) {
    try {
      await invoke("local_remove", { id });
      if (this.model === LOCAL_PREFIX + id) this.model = "";
      await this.loadLocal();
    } catch (e) {
      this.localError = String(e);
    }
  }

  async loadCaps(name: string) {
    if (!name || this.caps[name]) return;
    if (name.startsWith(LOCAL_PREFIX)) return; // renseignés par loadLocal()
    try {
      const info = await invoke<{ capabilities?: string[]; model_info?: Record<string, unknown> }>("model_show", { name, cloudKey: this.webKey });
      this.caps[name] = info.capabilities ?? [];
      this.ctxInfo[name] = parseCtxInfo(info.model_info);
    } catch (e) {
      // Les anciennes étiquettes cloud répondent « 410 Non disponible ».
      if (String(e).includes("410")) this.unavailable[name] = true;
    }
  }

  get supportsTools() {
    return this.caps[this.model]?.includes("tools") ?? true;
  }

  get supportsVision() {
    return this.caps[this.model]?.includes("vision") ?? false;
  }

  /** Capacités connues d'un modèle (vide tant qu'elles n'ont pas été lues). */
  capsOf(name: string): string[] {
    return (this.caps[name] ?? []).filter((c) => c in CAP_LABELS);
  }

  /** Modèles utilisables pour discuter : on écarte ceux qui ne font que de l'embedding. */
  get chatModels(): Model[] {
    return this.models.filter((m) => {
      const c = this.caps[m.name];
      return !c || c.includes("completion") || !c.includes("embedding");
    });
  }

  /** Lit les capacités de tous les modèles (quelques requêtes en parallèle) pour les afficher et filtrer. */
  async loadAllCaps() {
    const queue = this.models.map((m) => m.name).filter((n) => !this.caps[n]);
    await Promise.all(
      Array.from({ length: 4 }, async () => {
        for (let n = queue.shift(); n; n = queue.shift()) await this.loadCaps(n);
      }),
    );
  }

  async loadConvs() {
    this.convs = await invoke<Conv[]>("conv_list", {
      search: this.search,
      kind: this.mode,
      projectId: this.activeProject,
    });
  }

  async loadSkills() {
    try {
      this.skills = await invoke<Skill[]>("skills_list", { workspace: this.mode === "code" ? this.workspace : null });
    } catch {}
  }

  /** Importe les skills de Claude Desktop / Claude Code trouvés sur ce PC (une seule fois chacun). */
  private async autoImportSkills() {
    try {
      const n = await invoke<number>("skills_auto_import");
      if (n > 0) {
        await this.loadSkills();
        this.flash(`${n} skill${n > 1 ? "s" : ""} de Claude importé${n > 1 ? "s" : ""}.`);
      }
    } catch {}
  }

  async loadAgents() {
    try {
      this.agents = await invoke<AgentProfile[]>("agents_list", { workspace: this.mode === "code" ? this.workspace : null });
    } catch {}
  }

  /** Réponses données la dernière fois, pour préremplir si l'utilisateur refait sa présentation. */
  onboardingAnswers(): Partial<Answers> {
    try {
      return JSON.parse(load("onboardAnswers", "{}"));
    } catch {
      return {};
    }
  }

  /** Enregistre la présentation : les phrases remplacent les précédentes (même préfixe) sans toucher aux autres souvenirs. */
  /** Ouvre l'aperçu quand la réponse contient une page HTML complète ou une image SVG (comme les artefacts de Claude). */
  private openArtifact(text: string) {
    const a = lastOpenable(text);
    if (a) this.preview = { code: a.code, lang: a.lang };
  }

  finishTutorial(skipped = false) {
    if (!this.tutorialDone && !skipped) this.track({ kind: "tutorial" });
    this.tutorialDone = true;
    this.tutorialOpen = false;
    save("tutorial", "1");
  }

  async finishOnboarding(answers: Answers, facts: string[]) {
    try {
      await this.loadMemories();
      for (const m of this.memories.filter((m) => m.text.startsWith(FACT_PREFIX))) await invoke("memory_delete", { id: m.id });
      for (const f of facts) await invoke("memory_add", { text: f });
      await this.loadMemories();
      save("onboardAnswers", JSON.stringify(answers));
      this.onboardVersion++;
      this.flash(facts.length ? "Merci ! Lama se souviendra de tout ça." : "Présentation terminée.");
    } catch (e) {
      this.error = String(e);
    }
    this.skipOnboarding();
  }

  skipOnboarding() {
    this.onboarded = true;
    this.onboardingOpen = false;
    save("onboarded", "1");
  }

  async loadMemories() {
    this.memories = await invoke<Memory[]>("memory_list");
  }

  async addMemory(text: string) {
    if (!text.trim()) return;
    await invoke("memory_add", { text });
    this.track({ kind: "memory" });
    await this.loadMemories();
  }

  async loadProjectMemories(projectId: string) {
    try {
      this.projectMemories = await invoke<Memory[]>("memory_project_list", { projectId });
    } catch {
      this.projectMemories = [];
    }
  }

  async addProjectMemory(projectId: string, text: string) {
    if (!text.trim()) return;
    await invoke("memory_add", { text: text.trim(), projectId });
    this.track({ kind: "memory" });
    await this.loadProjectMemories(projectId);
  }

  async deleteProjectMemory(projectId: string, id: number) {
    await invoke("memory_delete", { id });
    await this.loadProjectMemories(projectId);
  }

  // ---- tâches planifiées ------------------------------------------------

  /** Compte un événement dans la progression ; félicite pour un niveau ou un badge gagné. */
  track(ev: ProgressEvent) {
    if (!this.gamify) return;
    const r = applyProgress($state.snapshot(this.progress), ev);
    this.progress = r.state;
    save("progress", JSON.stringify(r.state));
    if (r.gained) this.xpGain = { n: r.gained, at: Date.now() };
    for (const b of r.unlocked) this.celebrations.push({ id: ++this.celebId, kind: "badge", title: b.label, text: b.hint, icon: b.icon });
    if (r.levelUp) this.celebrations.push({ id: ++this.celebId, kind: "level", title: `Niveau ${r.levelUp} : ${titleOf(r.levelUp)}`, text: "Bravo, continue comme ça !", icon: "trophy" });
  }

  dismissCelebration(id: number) {
    this.celebrations = this.celebrations.filter((c) => c.id !== id);
  }

  openProgress() {
    this.celebrations = [];
    this.settingsTab = "progress";
    this.settingsWanted = true;
  }

  setGamify(v: boolean) {
    this.gamify = v;
    save("gamify", v ? "1" : "0");
    if (!v) this.celebrations = [];
  }

  resetProgress() {
    this.progress = JSON.parse(JSON.stringify(EMPTY_PROGRESS));
    save("progress", JSON.stringify(this.progress));
    this.celebrations = [];
  }

  async showTasks() {
    this.view = "tasks";
    await this.loadTasks();
  }

  async loadTasks() {
    try {
      const list = await invoke<ScheduledTask[]>("schedules_list");
      // Une tâche récurrente sans échéance (lancement interrompu) reprend son cours.
      for (const t of list) {
        if (t.enabled && t.next_at == null && t.repeat !== "once") {
          const n = nextRun(t);
          if (n) {
            await invoke("schedule_done", { id: t.id, nextAt: n, conv: null });
            t.next_at = n;
          }
        }
      }
      this.tasks = list;
    } catch {
      this.tasks = [];
    }
  }

  /** Crée ou modifie une tâche ; l'échéance est calculée ici, en heure locale. */
  async saveTask(t: Partial<ScheduledTask> & Pick<ScheduledTask, "name" | "prompt" | "repeat" | "time">) {
    const draft = { id: "", kind: "chat", workspace: null, project_id: null, weekday: 0, date: null, enabled: true, last_at: null, last_conv: null, ...t } as ScheduledTask;
    draft.next_at = draft.enabled ? nextRun(draft) : null;
    if (draft.enabled && draft.next_at == null) throw new Error("Cette date est déjà passée : choisis un moment à venir.");
    const isNew = !t.id;
    await invoke("schedule_save", { task: draft });
    if (isNew) this.track({ kind: "task" });
    await this.loadTasks();
  }

  async setTaskEnabled(t: ScheduledTask, enabled: boolean) {
    await this.saveTask({ ...$state.snapshot(t), enabled });
  }

  async deleteTask(id: string) {
    await invoke("schedule_delete", { id });
    await this.loadTasks();
  }

  /** Lance la tâche dans une nouvelle conversation, sans interrompre celle qui travaille (elle passe en arrière-plan). */
  async runTask(t: ScheduledTask, manual = false) {
    let conv: string | null = null;
    try {
      if (!this.model) throw new Error("aucun modèle n'est choisi");
      if (t.kind === "code" && !t.workspace) throw new Error("aucun dossier de travail");
      if (this.mode !== t.kind) await this.setMode(t.kind);
      else this.newChat();
      if (t.kind === "code") this.workspace = t.workspace ?? "";
      this.chatProject = t.project_id;
      this.alert("Tâche planifiée", t.name);
      await this.send(t.prompt);
      conv = this.fg.activeId;
      if (conv) {
        const stamp = new Date().toLocaleString("fr-FR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
        await invoke("conv_rename", { id: conv, title: `${t.name} — ${stamp}` });
        await this.loadConvs();
      }
    } catch (e) {
      this.flash(`Tâche « ${t.name} » non lancée : ${e}`);
    } finally {
      // Une tâche lancée à la main ne change pas son calendrier.
      if (!manual) {
        const next = t.repeat === "once" ? null : nextRun(t);
        await invoke("schedule_done", { id: t.id, nextAt: next, conv }).catch(() => {});
        await this.loadTasks();
      }
    }
  }

  async deleteMemory(id: number) {
    await invoke("memory_delete", { id });
    await this.loadMemories();
  }

  async loadProjects() {
    this.projects = await invoke<Project[]>("project_list");
  }

  /** Projets du mode affiché : les projets de chat d'un côté, ceux de code (liés à un dossier) de l'autre. */
  get modeProjects(): Project[] {
    return this.projects.filter((p) => p.kind === this.mode);
  }

  get currentProject(): Project | undefined {
    return this.projects.find((p) => p.id === this.chatProject);
  }

  async loadArtifacts() {
    try {
      this.artifacts = await invoke<SavedArtifact[]>("artifact_list", { search: "" });
    } catch {}
  }

  async showArtifacts() {
    this.view = "artifacts";
    await this.loadArtifacts();
  }

  async deleteArtifact(a: SavedArtifact) {
    await invoke("artifact_delete", { id: a.id });
    await this.loadArtifacts();
  }

  /** Rouvre un artefact de la bibliothèque : dans sa conversation d'origine si elle existe encore, avec ses versions. */
  async openSavedArtifact(a: SavedArtifact) {
    const convs = a.conv_id ? await invoke<Conv[]>("conv_list", { search: "", kind: "chat", projectId: null }).catch(() => []) : [];
    const origin = convs.find((c) => c.id === a.conv_id);
    if (origin) await this.open(origin);
    else this.newChat();
    this.view = "chat";
    this.preview = { code: a.code, lang: a.lang };
  }

  /** Démarre une création (document, diapositives, design, tableau de bord) : nouveau chat avec l'invite de départ. */
  startCreation(kind: CreationKind) {
    this.newChat();
    this.creating = kind;
    this.draft = CREATIONS[kind].prompt;
  }

  /** Garde le dernier artefact de la réponse dans la bibliothèque. */
  private async saveArtifact(s: Session, text: string) {
    const a = lastOpenable(text);
    if (!a || !s.activeId) return;
    try {
      const saved = await invoke<{ versions: number }>("artifact_save", { convId: s.activeId, title: a.title, lang: a.lang, kind: s.artifactKind, code: a.code });
      if (saved.versions === 1) this.track({ kind: "artifact" });
    } catch {}
  }

  showProjects() {
    this.view = "projects";
  }

  /** Ouvre la page d'un projet : zone de saisie pour une nouvelle conversation, instructions, documents, chats récents. */
  /** Filtre la liste des conversations sur un projet, sans quitter la conversation affichée. */
  async filterProject(id: string | null) {
    this.activeProject = id;
    await this.loadConvs();
  }

  async openProject(id: string) {
    await this.selectProject(id);
    const p = this.projects.find((x) => x.id === id);
    if (p?.workspace && this.mode === "code") this.workspace = p.workspace;
    this.view = "project";
  }

  async pinProject(p: Project) {
    await invoke("project_pin", { id: p.id, pinned: !p.pinned });
    await this.loadProjects();
  }

  async selectProject(id: string | null) {
    this.activeProject = id;
    this.newChat();
    await this.loadConvs();
  }

  private async syncDocs(projectId: string, docs: DocDraft[], removed: string[]) {
    for (const id of removed) await invoke("project_doc_delete", { id });
    for (const d of docs.filter((d) => !d.id)) {
      await invoke("project_doc_add", { projectId, name: d.name, content: d.content });
    }
  }

  async createProject(name: string, instructions: string, docs: DocDraft[] = [], workspace = "") {
    const p = await invoke<Project>("project_create", { name: name.trim(), instructions, kind: this.mode, workspace: workspace || null });
    await this.syncDocs(p.id, docs, []);
    this.track({ kind: "project" });
    await this.loadProjects();
    await this.openProject(p.id);
  }

  async updateProject(id: string, name: string, instructions: string, docs: DocDraft[] = [], removed: string[] = [], workspace = "") {
    await invoke("project_update", { id, name: name.trim(), instructions, workspace: workspace || null });
    await this.syncDocs(id, docs, removed);
    await this.loadProjects();
  }

  async deleteProject(id: string) {
    await invoke("project_delete", { id });
    if (this.activeProject === id) {
      this.activeProject = null;
      if (this.view === "project") this.view = "projects";
    }
    if (this.chatProject === id) this.chatProject = null;
    await this.loadProjects();
    await this.loadConvs();
  }

  async moveToProject(c: Conv, projectId: string | null) {
    await invoke("conv_set_project", { id: c.id, projectId });
    if (c.id === this.activeId) this.chatProject = projectId;
    await this.loadConvs();
  }

  async renameConv(c: Conv, title: string) {
    title = title.trim();
    if (!title || title === c.title) return;
    await invoke("conv_rename", { id: c.id, title });
    await this.loadConvs();
  }

  /** Démarre les écouteurs d'events et le polling ; renvoie la fonction de nettoyage. */
  init() {
    this.applyTheme();
    this.loadCloud();
    this.loadLocal();
    this.loadProjects();
    this.loadConvs();
    this.loadSkills().then(() => this.autoImportSkills());
    this.loadAgents();
    this.loadMemories();
    this.loadTasks();
    const unsubs = Promise.all([
      listen<{ id: string; chunk: any }>("chat:chunk", async (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        this.queueChunk(s, e.payload.chunk.content ?? "", e.payload.chunk.thinking ?? "");
        if (!e.payload.chunk.done) return;
        this.flushChunks(s);
        s.busy = false;
        await this.persist(s);
        const last = s.messages[s.messages.length - 1];
        void this.saveArtifact(s, last?.content ?? "");
        const text = (last?.content ?? "").replace(/\s+/g, " ").trim() || "Lama Desktop";
        if (s !== this.fg) {
          // Conversation restée en arrière-plan : elle a fini pendant qu'on regardait autre chose.
          this.unpark(s);
          const title = this.titleOf(s);
          this.alert(`Terminé : ${title}`, text);
          this.track({ kind: "answer" });
          this.flash(`« ${title} » a terminé.`);
          return;
        }
        this.openArtifact(last?.content ?? "");
        this.track({ kind: "answer" });
        if (this.mode === "code" && this.sessionChanges > this.changesBeforeRun) this.track({ kind: "code" });
        if (this.mode === "code" && this.autoReview && this.sessionChanges > this.changesBeforeRun) void this.reviewChanges(true);
        this.alert(this.mode === "code" ? "Tâche terminée" : "Réponse prête", text);
        // Une précision arrivée trop tard (ou pour un modèle sans outils) part comme un message normal.
        if (s.queued.length) {
          const next = s.queued.join("\n\n");
          s.queued = [];
          void this.send(next);
        }
      }),
      listen<{ id: string; text: string }>("agent:injected", async (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        const i = s.queued.indexOf(e.payload.text);
        if (i >= 0) s.queued.splice(i, 1);
        this.flushChunks(s);
        const cur = s.messages[s.messages.length - 1];
        if (cur?.role === "assistant" && !cur.content && !cur.thinking && !cur.tools?.length) s.messages.pop();
        s.messages.push({ role: "user", content: e.payload.text });
        s.messages.push({ role: "assistant", content: "" });
        if (s === this.fg) await this.scrollDown();
      }),
      listen<{ id: string }>("agent:step", (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        this.flushChunks(s);
        const last = s.messages[s.messages.length - 1];
        if (last?.role !== "assistant") return;
        // Le texte de l'étape qui vient de finir devient un commentaire, placé avant les outils qu'elle a lancés.
        if (last.content.trim()) {
          last.notes ??= [];
          last.notes.push({ text: last.content.trim(), at: last.stepStart ?? 0 });
          last.content = "";
        }
        last.stepStart = last.tools?.length ?? 0;
      }),
      listen("emergency:stop", () => {
        // Arrêt d'urgence (raccourci global ou icône de la zone de notification) : toutes les conversations s'interrompent.
        for (const s of [this.fg, ...this.parked]) {
          this.dropChunks(s);
          s.busy = false;
          const cut = s.messages[s.messages.length - 1];
          if (cut?.role === "assistant") cut.interrupted = true;
          s.messages.forEach((m) => m.tools?.forEach((t) => {
            if (t.state === "awaiting" || t.state === "running") t.state = "denied";
          }));
          if (s !== this.fg) void this.persist(s);
        }
        this.parked = [];
        this.returnQueued();
        this.flash("Arrêt d'urgence : tout est arrêté.");
      }),
      listen<{ id?: string; prompt: number; eval: number }>("usage:tokens", (e) => {
        // Les événements sont diffusés à toutes les fenêtres : on ne compte que ceux de ses propres requêtes.
        const s = e.payload.id ? this.sess(e.payload.id) : undefined;
        if (s) this.addUsage(e.payload.prompt, e.payload.eval, s.model);
      }),
      listen<LocalProgress>("local:progress", (e) => (this.localProgress = e.payload)),
      listen<{ id: string }>("local:done", async () => {
        this.localProgress = null;
        await this.loadLocal();
      }),
      listen<{ id: string; error: string }>("local:error", (e) => {
        this.localProgress = null;
        this.localError = e.payload.error;
      }),
      listen<ScheduledTask>("schedule:due", (e) => void this.runTask(e.payload)),
      listen<{ id: string; prompt_tokens: number }>("chat:usage", (e) => {
        const s = this.sess(e.payload.id);
        if (s) s.promptTokens = e.payload.prompt_tokens;
      }),
      listen<{ id: string; error: string }>("chat:error", async (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        this.flushChunks(s);
        s.error = e.payload.error;
        s.busy = false;
        const failed = s.messages[s.messages.length - 1];
        if (failed?.role === "assistant") failed.interrupted = true;
        if (s === this.fg) this.returnQueued();
        else {
          this.unpark(s);
          this.alert(`Erreur : ${this.titleOf(s)}`, e.payload.error);
        }
        await this.persist(s);
      }),
      listen<{ id: string; tasks: Task[] }>("agent:tasks", async (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        s.messages[s.messages.length - 1].tasks = e.payload.tasks;
        if (s === this.fg) await this.scrollDown();
      }),
      listen<ToolEvent & { id: string }>("agent:tool", async (e) => {
        const s = this.sess(e.payload.id);
        if (!s) return;
        const last = s.messages[s.messages.length - 1];
        const { id: _id, ...ev } = e.payload;
        last.tools ??= [];
        const i = last.tools.findIndex((t) => t.call_id === ev.call_id);
        if (ev.state === "done" && isWrite(ev.name) && last.tools[i]?.state !== "done") this.sessionChanges++;
        if (ev.state === "awaiting") this.alert(s === this.fg ? "Autorisation requise" : `Autorisation requise : ${this.titleOf(s)}`, `${ev.name} ${String(ev.args.path ?? ev.args.command ?? ev.args.to ?? "")}`);
        if (i >= 0) last.tools[i] = ev;
        else last.tools.push(ev);
        this.autosave(s);
        if (s === this.fg) await this.scrollDown();
      }),
    ]);
    return () => {
      unsubs.then((fns) => fns.forEach((f) => f()));
    };
  }

  async scrollDown() {
    await tick();
    this.scroller?.scrollTo({ top: this.scroller.scrollHeight });
  }

  /** La conversation sous la forme enregistrée (base de données, export, notes). */
  private storedMessages(s: Session = this.fg) {
    return $state
      .snapshot(s.messages)
      .filter((m) => m.content || m.tools?.length || m.tasks?.length || m.notes?.length || m.role === "user")
      .map((m, i, all) => {
        // Pendant une tâche, la dernière réponse est enregistrée comme « interrompue » : si l'application se ferme, on pourra la reprendre.
        const interrupted = m.interrupted || (s.busy && m.role === "assistant" && i === all.length - 1);
        return {
          role: m.role,
          content: m.content,
          thinking: m.thinking ?? null,
          extra: m.tools?.length || m.attachments?.length || m.tasks?.length || m.notes?.length || interrupted || m.skipWire
            ? JSON.stringify({ tools: m.tools, attachments: m.attachments, tasks: m.tasks, notes: m.notes, interrupted: interrupted || undefined, skipWire: m.skipWire || undefined })
            : null,
        };
      });
  }

  /**
   * `/memoire` : condense la conversation en quelques lignes et les ajoute à la mémoire du profil, relue par le modèle
   * dans les conversations suivantes. Refaire `/memoire` sur la même conversation remplace la ligne précédente.
   */
  async saveToMemory(titleHint = "") {
    if (this.busy || this.compacting || this.savingMemory) return;
    const stored = this.storedMessages();
    if (stored.filter((m) => m.content.trim()).length < 2) {
      this.flash("Rien à retenir : la conversation est vide.");
      return;
    }
    this.savingMemory = true;
    this.flash("Compactage de la conversation dans ta mémoire…");
    try {
      // Même budget que le compactage : les messages les plus récents d'abord, pour tenir dans le contexte du modèle.
      let budget = Math.floor(this.ctxLimit * 2.5);
      const lines: string[] = [];
      for (const m of [...stored].reverse()) {
        const line = `${m.role === "user" ? "Utilisateur" : m.role === "system" ? "Résumé précédent" : "Assistant"} : ${m.content.slice(0, 4000)}`;
        if (budget - line.length < 0 && lines.length) break;
        budget -= line.length;
        lines.unshift(line);
      }
      const out = await invoke<string>("chat_once", {
        model: this.model,
        cloudKey: this.webKey,
        options: { num_ctx: this.ctxLimit },
        messages: [{ role: "system", content: MEMORY_PROMPT }, { role: "user", content: lines.join("\n\n") }],
      });
      const { title, points } = parseMemory(out);
      if (!points) throw new Error("Le modèle n'a rien retenu de cette conversation : réessaie.");
      const date = new Date().toLocaleDateString("fr-FR");
      const added = await invoke<Memory>("memory_add", { text: memoryLine(titleHint.trim() || title, date, points) });
      // Une seule ligne par conversation : l'ancienne version est remplacée.
      const ids: Record<string, number> = JSON.parse(load("memoireIds", "{}"));
      const key = this.activeId ?? "";
      if (key && ids[key]) await invoke("memory_delete", { id: ids[key] }).catch(() => {});
      if (key) {
        ids[key] = added.id;
        save("memoireIds", JSON.stringify(ids));
      }
      await this.loadMemories();
      this.flash("Conversation compactée dans ta mémoire.");
    } catch (e) {
      this.error = String(e instanceof Error ? e.message : e);
    } finally {
      this.savingMemory = false;
    }
  }

  private titleOf(s: Session) {
    return this.convs.find((c) => c.id === s.activeId)?.title ?? "Conversation";
  }

  /** Enregistre la conversation pendant la tâche (au plus toutes les 2 s) pour pouvoir la reprendre après une coupure. */
  private autosave(s: Session) {
    if (s.saveTimer || !s.activeId) return;
    s.saveTimer = setTimeout(() => {
      s.saveTimer = null;
      if (s.busy) void this.persist(s);
    }, 2000);
  }

  /** Une tâche a été coupée : on peut la reprendre (le dernier message est une réponse interrompue ou une question restée sans réponse). */
  get resumable(): boolean {
    if (this.busy || this.compacting || !this.activeId) return false;
    const last = this.messages[this.messages.length - 1];
    return !!last && ((last.role === "assistant" && !!last.interrupted) || (last.role === "user" && !this.error));
  }

  get resumeKind(): "cut" | "unanswered" {
    return this.messages[this.messages.length - 1]?.role === "assistant" ? "cut" : "unanswered";
  }

  /** Reprend là où la tâche s'était arrêtée : le modèle reçoit un récapitulatif de ce qui est fait. */
  async resumeTask() {
    if (!this.resumable) return;
    this.error = "";
    const last = this.messages[this.messages.length - 1];
    if (last.role === "assistant") {
      this.turnSystem.push(resumeNote($state.snapshot(last)));
      last.interrupted = false;
      last.skipWire = true;
    }
    try {
      await this.run();
    } catch (e) {
      this.error = String(e);
      this.busy = false;
    }
  }

  private async persist(s: Session = this.fg) {
    if (!s.activeId) return;
    const msgs = this.storedMessages(s);
    await invoke("conv_save", { id: s.activeId, model: s.model || this.model, messages: msgs });
    await this.loadConvs();
  }

  async setMode(mode: Mode) {
    if (mode === this.mode) return;
    this.mode = mode;
    this.activeProject = null;
    this.search = "";
    this.newChat();
    await this.loadConvs();
  }

  newChat() {
    this.view = "chat";
    this.creating = null;
    this.detach();
    this.pending = [];
    this.preview = null;
    this.chatProject = this.activeProject;
    if (this.mode === "chat") this.workspace = "";
  }

  get canSend() {
    return !!this.model && (this.mode === "chat" || !!this.workspace.trim());
  }

  // ---- pièces jointes -------------------------------------------------

  async addPaths(paths: string[]) {
    for (const path of paths) {
      try {
        let att = await invoke<Attachment>("read_attachment", { path });
        if (att.kind === "image" && this.caps[this.model] && !this.supportsVision) {
          this.error = `« ${this.model} » ne gère pas les images : choisis un modèle avec la vision (voir Modèles, Ctrl+M).`;
          continue;
        }
        if (att.kind === "image") att = { ...att, data: await shrinkImage(att.data) };
        else this.addText(att);
        if (att.kind === "image") this.pending.push(att);
      } catch (e) {
        this.error = String(e);
      }
    }
  }

  /** Fichier texte joint : poids estimé, et envoi en extraits s'il est lourd. */
  private addText(att: Attachment) {
    const a = prepare(att);
    this.pending.push(a);
    if (a.mode === "excerpt") {
      this.flash(`« ${a.name} » pèse environ ${formatTokens(a.tokens ?? 0)} jetons : seuls des extraits sont envoyés, le modèle lit la suite à la demande (bouton « Extraits » pour tout envoyer).`);
    }
  }

  /** Image collée (Ctrl+V) : seulement si le modèle sait lire les images. */
  async addPastedImage(file: Blob, name = "image collée.png"): Promise<boolean> {
    if (!this.supportsVision) {
      this.flash(`« ${this.model} » ne gère pas les images : choisis un modèle avec la vision pour coller une image.`);
      return false;
    }
    try {
      const data = await shrinkImage(await fileToBase64(file));
      this.pending.push({ name, kind: "image", data });
      return true;
    } catch (e) {
      this.error = String(e);
      return false;
    }
  }

  async pickFiles() {
    const sel = await openDialog({ multiple: true, title: "Joindre des fichiers" });
    if (sel) await this.addPaths(Array.isArray(sel) ? sel : [sel]);
  }

  async pickFolder() {
    const sel = await openDialog({ directory: true, title: "Choisir le dossier de travail" });
    if (typeof sel === "string") this.workspace = sel;
  }

  // ---- envoi, régénération, édition ------------------------------------

  /** Traite `/commande`. Renvoie true si le message ne doit pas être envoyé au modèle. */
  private async slash(text: string): Promise<boolean> {
    const m = /^\/([\p{L}\p{N}_-]+)\s*([\s\S]*)$/u.exec(text);
    if (!m) return false;
    const [, cmd, rest] = m;
    if (cmd === "clear") {
      this.newChat();
      return true;
    }
    if (cmd === "compact") {
      await this.compact();
      return true;
    }
    if (cmd === "recherche") {
      if (!rest.trim()) this.flash("Précise la question : /recherche …");
      else {
        this.researchNext = true;
        await this.send(rest.trim());
      }
      return true;
    }
    if (cmd === "memoire" || cmd === "mémoire") {
      await this.saveToMemory(rest);
      return true;
    }
    if (cmd === "remember") {
      if (rest.trim()) {
        await this.addMemory(rest);
        this.flash("Mémorisé.");
      } else this.flash("Précise quoi retenir : /remember …");
      return true;
    }
    const skill = this.skills.find((s) => s.name.toLowerCase() === cmd.toLowerCase());
    if (skill) {
      try {
        const body = await invoke<string>("skill_read", { workspace: this.mode === "code" ? this.workspace : null, name: skill.name, args: rest.trim() || null });
        this.turnSystem.push(`L'utilisateur active le skill « ${skill.name} ». Suis ces instructions :\n\n${body}`);
      } catch (e) {
        this.error = String(e);
        return true;
      }
      return false;
    }
    return false;
  }

  /** Résume les anciens messages pour libérer du contexte. */
  async compact(): Promise<void> {
    if (this.busy || this.compacting) return;
    const keep = 4;
    const old = this.messages.slice(0, Math.max(0, this.messages.length - keep));
    if (old.length < 2) {
      this.flash("Conversation trop courte pour être compactée.");
      return;
    }
    this.compacting = true;
    try {
      // Le texte à résumer doit tenir dans le contexte du modèle : on garde le plus récent d'abord.
      let budget = Math.floor(this.ctxLimit * 2.5);
      const lines: string[] = [];
      for (const m of $state.snapshot(old).reverse()) {
        const line = `${m.role === "user" ? "Utilisateur" : m.role === "system" ? "Résumé précédent" : "Assistant"} : ${m.content.slice(0, 4000)}`;
        if (budget - line.length < 0 && lines.length) break;
        budget -= line.length;
        lines.unshift(line);
      }
      const transcript = lines.join("\n\n");
      const summary = await invoke<string>("chat_once", {
        model: this.model,
        cloudKey: this.webKey,
        options: { num_ctx: this.ctxLimit },
        messages: [
          {
            role: "system",
            content:
              "Résume fidèlement cette conversation en français pour la continuer plus tard : faits, décisions, préférences, code et fichiers importants. Sois concis, sans introduction.",
          },
          { role: "user", content: transcript },
        ],
      });
      // Les instructions des skills chargés disparaissent avec les anciens messages : on rappelle lesquels le modèle peut recharger.
      const used = [...new Set(old.flatMap((m) => (m.tools ?? []).filter((t) => t.name === "use_skill").map((t) => String(t.args.name))))];
      const skillNote = used.length ? `\n\nSkills déjà utilisés dans cette conversation (recharge-les avec use_skill si besoin) : ${used.join(", ")}.` : "";
      this.messages = [
        { role: "system", content: `Résumé de la conversation précédente :\n${summary.trim()}${skillNote}` },
        ...this.messages.slice(old.length),
      ];
      this.promptTokens = 0;
      await this.persist();
      this.flash("Conversation compactée.");
    } catch (e) {
      this.error = String(e);
    } finally {
      this.compacting = false;
    }
  }

  /** Vrai quand la conversation approche de la limite du modèle (mesure réelle, sinon estimation). */
  private needsCompaction(): boolean {
    if (this.messages.length <= 6) return false;
    const used = this.promptTokens || estimateTokens(this.messages.map((m) => m.content).join("\n"));
    return used > COMPACT_AT * this.ctxLimit;
  }

  private turnSystem: string[] = [];

  /** Précisions envoyées pendant que le modèle travaille : prises en compte à la prochaine étape, sans rien casser. */
  async enqueue(text: string) {
    text = text.trim();
    if (!text || text.startsWith("/")) return;
    this.queued.push(text);
    if (this.fg.agentRun) await invoke("agent_inject", { requestId: this.fg.currentId, text }).catch(() => {});
  }

  /** Rend à la zone de saisie les précisions que la tâche n'a pas eu le temps de lire. */
  private returnQueued() {
    if (!this.queued.length) return;
    this.draft = this.queued.join("\n\n");
    this.queued = [];
  }

  async send(text: string) {
    if (this.busy) return this.enqueue(text);
    text = text.trim();
    if ((!text && !this.pending.length) || this.busy || this.compacting || this.savingMemory || !this.canSend) return;
    const s = this.fg;
    s.error = "";
    this.view = "chat";
    if (this.pending.some((a) => a.kind === "image") && this.caps[this.model] && !this.supportsVision) {
      s.error = `« ${this.model} » ne gère pas les images : retire-les ou choisis un modèle avec la vision.`;
      return;
    }
    this.turnSystem = [];
    if (this.creating) {
      this.turnSystem.push(CREATIONS[this.creating].hint);
      s.artifactKind = this.creating;
      this.creating = null;
    }
    if (await this.slash(text)) return;
    if (this.needsCompaction()) {
      this.flash("Contexte presque plein : compactage automatique…");
      await this.compact();
    }
    try {
      if (!s.activeId) {
        const c = await invoke<Conv>("conv_create", {
          model: this.model,
          kind: this.mode,
          workspace: this.mode === "code" ? this.workspace.trim() : null,
          projectId: this.chatProject,
        });
        s.activeId = c.id;
        const title = text || this.pending[0]?.name || "Nouvelle conversation";
        await invoke("conv_rename", { id: c.id, title: title.length > 40 ? title.slice(0, 40) + "…" : title });
      }
      const attachments = this.pending.length ? $state.snapshot(this.pending) : undefined;
      this.pending = [];
      s.messages.push({ role: "user", content: text, attachments });
      this.track({ kind: "message" });
      await this.run(s);
    } catch (e) {
      s.error = String(e);
      s.busy = false;
    }
  }

  /** Relance le modèle sur l'historique de cette conversation (le dernier message est celui de l'utilisateur). */
  private async run(s: Session = this.fg) {
    const history = wireHistory($state.snapshot(s.messages), this.supportsTools);
    for (const t of this.turnSystem) history.unshift({ role: "system", content: t });
    this.turnSystem = [];
    const deep = this.researchNext;
    this.researchNext = false;
    if (deep) history.unshift({ role: "system", content: RESEARCH_PROMPT });

    // Contexte permanent : projet (instructions + documents) puis mémoire.
    const project = this.currentProject;
    let indexed: { name: string; content: string }[] = [];
    if (project) {
      const docs = await invoke<ProjectDoc[]>("project_docs", { projectId: project.id });
      const parts = [project.instructions.trim()];
      if (this.supportsTools) {
        // Avec outils : seuls les petits documents sont envoyés en entier ; les autres sont indexés et le modèle y cherche des passages.
        const split = splitProjectDocs(docs);
        for (const d of split.inline) parts.push(`[Document du projet : ${d.name}]\n${d.content}`);
        if (split.indexed.length) {
          indexed = split.indexed.map((d) => ({ name: d.name, content: d.content }));
          parts.push(
            "Documents du projet consultables avec l'outil search_project_docs (cherche-y la réponse plutôt que de deviner) :\n" +
              split.indexed.map((d) => `- ${d.name} (${d.content.length} caractères) : ${d.content.replace(/\s+/g, " ").slice(0, 160)}…`).join("\n"),
          );
        }
      } else {
        let budget = 40_000;
        for (const d of docs) {
          const body = d.content.slice(0, Math.max(0, budget));
          budget -= body.length;
          if (body) parts.push(`[Document du projet : ${d.name}]\n${body}`);
        }
      }
      const text = parts.filter(Boolean).join("\n\n");
      if (text) history.unshift({ role: "system", content: text });
      const mem = await invoke<Memory[]>("memory_project_list", { projectId: project.id }).catch(() => [] as Memory[]);
      if (mem.length) history.unshift({ role: "system", content: `Mémoire du projet « ${project.name} » :\n` + mem.map((m) => `- ${m.text}`).join("\n") });
    }
    if (this.memories.length) {
      history.unshift({
        role: "system",
        content: "À propos de l'utilisateur (mémoire) :\n" + this.memories.map((m) => `- ${m.text}`).join("\n"),
      });
    }

    const planning = this.permission === "plan" && this.models.some((m) => m.name === this.planModel);
    const chosen = planning ? this.planModel : this.model;
    await this.loadCaps(chosen);
    let options: object | null = { num_ctx: this.ctxFor(chosen) };
    const tools = this.supportsTools;
    // Tout passe par la boucle agent quand le modèle gère les outils : skills, notes, recherche web et délégation
    // à des sous-agents sont alors disponibles partout, et le modèle décide seul de s'en servir.
    const agent = tools;
    if (deep && !tools) {
      s.error = `« ${chosen} » ne gère pas les outils, nécessaires à la recherche approfondie : choisis un modèle avec « tools » (Ctrl+M).`;
      return;
    }
    if (this.mode === "code") {
      const req = codeRequest(this.effort, this.permission);
      options = { ...req.options, num_ctx: this.ctxFor(chosen) };
      if (req.system) history.unshift({ role: "system", content: req.system });
    }
    if (this.mode === "chat" && this.permission === "plan") history.unshift({ role: "system", content: PLAN_HINT });
    if (this.think !== "auto" && this.caps[chosen]?.includes("thinking")) {
      options = { ...options, think: thinkValue(chosen, this.think === "on", this.effort) };
    }
    s.messages.push({ role: "assistant", content: "" });
    s.busy = true;
    s.agentRun = agent;
    s.model = chosen;
    this.changesBeforeRun = this.sessionChanges;
    s.currentId = crypto.randomUUID();
    const base = { requestId: s.currentId, model: chosen, cloudKey: this.webKey, messages: history, options };
    if (agent) {
      await invoke("agent_start", {
        ...base,
        workspace: this.mode === "code" ? this.workspace.trim() : null,
        mode: this.permission,
        web: this.webSearch || deep,
        attachments: attachmentsForTool($state.snapshot(s.messages)),
        docs: indexed.length ? indexed : null,
        projectId: project?.id ?? null,
      });
    } else {
      await invoke("chat_start", base);
    }
    await this.scrollDown();
  }

  async regenerate() {
    if (this.busy) return;
    const lastUser = this.messages.findLastIndex((m) => m.role === "user");
    if (lastUser < 0) return;
    this.error = "";
    this.messages.splice(lastUser + 1);
    try {
      await this.run();
    } catch (e) {
      this.error = String(e);
      this.busy = false;
    }
  }

  async editMessage(index: number, text: string) {
    const orig = this.messages[index];
    if (this.busy || !orig || orig.role !== "user" || !text.trim()) return;
    this.error = "";
    this.messages.splice(index);
    this.messages.push({ role: "user", content: text.trim(), attachments: orig.attachments });
    try {
      await this.run();
    } catch (e) {
      this.error = String(e);
      this.busy = false;
    }
  }

  async stop() {
    const s = this.fg;
    await invoke("chat_cancel", { requestId: s.currentId });
    this.flushChunks(s);
    s.busy = false;
    this.returnQueued();
    const last = s.messages[s.messages.length - 1];
    if (last?.role === "assistant") last.interrupted = true;
    // Les actions en attente d'approbation ne seront plus jamais exécutées.
    last?.tools?.forEach((t) => {
      if (t.state === "awaiting" || t.state === "running") t.state = "denied";
    });
    await this.persist(s);
  }

  async approve(call_id: string, allow: boolean) {
    await invoke("agent_approve", { callId: call_id, allow });
  }

  /** Annule la dernière modification de l'agent ; renvoie un message pour l'utilisateur. */
  async undo(): Promise<string> {
    try {
      const path = await invoke<string>("agent_undo");
      this.sessionChanges = Math.max(0, this.sessionChanges - 1);
      return `Annulé : ${path}`;
    } catch (e) {
      return String(e);
    }
  }

  /** Annule, de la plus récente à la plus ancienne, les modifications faites dans un message. */
  async undoMessage(index: number) {
    const msg = this.messages[index];
    const writes = (msg?.tools ?? []).filter((t) => isWrite(t.name) && t.state === "done").reverse();
    for (const t of writes) {
      try {
        await invoke("agent_undo");
        this.sessionChanges = Math.max(0, this.sessionChanges - 1);
        t.state = "undone";
      } catch (e) {
        this.error = String(e);
        break;
      }
    }
    await this.persist();
  }

  async open(c: Conv) {
    // Déjà affichée : rien à faire ; sinon la conversation en cours (si elle travaille) passe en arrière-plan.
    this.view = "chat";
    if (this.fg.activeId === c.id) return;
    this.detach();
    const running = this.parked.find((p) => p.activeId === c.id);
    if (running) {
      this.unpark(running);
      this.fg = running;
    } else {
      this.fg.activeId = c.id;
    }
    const s = this.fg;
    this.pending = [];
    this.preview = null;
    this.workspace = c.workspace ?? "";
    this.chatProject = c.project_id;
    this.loadSkills();
    this.loadAgents();
    if (!running) s.messages = (await invoke<Stored[]>("conv_messages", { id: c.id })).map(fromStored);
    if (this.models.some((m) => m.name === cleanModel(c.model))) this.model = cleanModel(c.model);
    await this.scrollDown();
  }

  async pin(c: Conv) {
    await invoke("conv_pin", { id: c.id, pinned: !c.pinned });
    await this.loadConvs();
  }

  /** Branche : copie la conversation jusqu'à ce message (exclu) dans une nouvelle conversation, avec le message à réécrire dans la zone de saisie. */
  async branchFrom(index: number) {
    if (this.busy || !this.activeId) return;
    const text = this.messages[index]?.content ?? "";
    try {
      await this.persist();
      const conv = await invoke<Conv>("conv_fork", { id: this.activeId, upto: index });
      await this.loadConvs();
      await this.open(conv);
      this.draft = text;
      this.flash("Branche créée : modifie ta question puis envoie-la. L'original est conservé.");
    } catch (e) {
      this.error = String(e);
    }
  }

  async remove(c: Conv) {
    await invoke("conv_delete", { id: c.id });
    if (this.activeId === c.id) this.newChat();
    await this.loadConvs();
  }
}
