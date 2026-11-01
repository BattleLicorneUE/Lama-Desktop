/** Une conversation et son état de travail. */

import type { Msg } from "$lib/chat-types";

/**
 * Une conversation avec son état de travail. Celle qu'on regarde est `fg` ; quand on en ouvre une autre pendant
 * qu'elle travaille, elle passe en arrière-plan (`parked`) et continue sans être interrompue.
 */
export class Session {
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
