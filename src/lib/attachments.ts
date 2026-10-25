/** Pièces jointes et historique : tout ce qui réduit les jetons envoyés au modèle sans lui cacher l'essentiel. */
import type { Attachment, Msg } from "$lib/chat.svelte";

const FENCE = "`".repeat(3);
const tokensOf = (s: string) => Math.ceil(s.length / 3.5);

/** Au-delà de ce poids (en jetons), un fichier texte est joint en extrait : le modèle lit la suite à la demande. */
export const HEAVY_TOKENS = 6000;
const EXCERPT_HEAD = 6000;
const EXCERPT_TAIL = 1500;

/** Messages récents conservés entiers ; au-delà, les pièces jointes et les longs textes sont allégés. */
const KEEP_FILES_TURNS = 3;
const KEEP_IMAGES_TURNS = 2;
const KEEP_FULL_MESSAGES = 10;
const OLD_MESSAGE_MAX = 4000;

/** Réduit une image (côté le plus long : 1568 px, JPEG) avant de l'envoyer : un écran entier passe de plusieurs Mo à quelques centaines de Ko. */
export async function shrinkImage(base64: string, max = 1568): Promise<string> {
  try {
    const blob = await (await fetch(`data:image/*;base64,${base64}`)).blob();
    const bmp = await createImageBitmap(blob);
    const scale = Math.min(1, max / Math.max(bmp.width, bmp.height));
    if (scale === 1 && base64.length < 300_000) return base64;
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bmp.width * scale));
    canvas.height = Math.max(1, Math.round(bmp.height * scale));
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
    const out = canvas.toDataURL("image/jpeg", 0.85).split(",")[1];
    return out && out.length < base64.length ? out : base64;
  } catch {
    return base64;
  }
}

/** Fichier lu depuis le presse-papiers ou le disque, en base64 (sans l'en-tête `data:`). */
export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((ok, fail) => {
    const r = new FileReader();
    r.onload = () => ok(String(r.result).split(",")[1] ?? "");
    r.onerror = () => fail(r.error);
    r.readAsDataURL(file);
  });
}

/** Titres repérés dans un texte (Markdown, numérotation, MAJUSCULES) : un plan pour s'orienter dans un extrait. */
export function outline(text: string, max = 30): string[] {
  const out: string[] = [];
  for (const raw of text.split("\n")) {
    const t = raw.trim();
    if (t.length < 3 || t.length > 90) continue;
    const heading =
      /^#{1,4}\s/.test(t) ||
      (/^(\d+(\.\d+)*[.)]?|[IVX]+\.|Chapitre|Section|Partie|Article)\s+\S/i.test(t) && !/[.;,]$/.test(t)) ||
      /^[A-ZÀ-Ý][A-ZÀ-Ý0-9 '’\-:]{5,}$/.test(t);
    if (heading) out.push(t);
    if (out.length >= max) break;
  }
  return out;
}

/** Texte d'un fichier lourd : le début, la fin et un plan, avec le moyen de lire le reste. */
export function excerpt(a: Attachment): string {
  const t = a.data;
  const n = t.length;
  const heads = outline(t);
  const plan = heads.length ? `Plan repéré :\n${heads.map((h) => "- " + h).join("\n")}\n\n` : "";
  return (
    `[Fichier : ${a.name} — ${n} caractères, environ ${tokensOf(t)} jetons. Seul un extrait est joint : lis le reste avec l'outil read_attachment ` +
    `(paramètre search pour un mot, ou start et length pour une tranche).]\n${plan}Début :\n${FENCE}\n${t.slice(0, EXCERPT_HEAD)}\n${FENCE}\n` +
    `[… ${Math.max(0, n - EXCERPT_HEAD - EXCERPT_TAIL)} caractères omis …]\nFin :\n${FENCE}\n${t.slice(-EXCERPT_TAIL)}\n${FENCE}`
  );
}

/** Préparation d'une pièce jointe lue : poids estimé et mode d'envoi par défaut (extraits si elle est lourde). */
export function prepare(a: Attachment): Attachment {
  if (a.kind !== "text") return a;
  const tokens = tokensOf(a.data);
  return { ...a, tokens, mode: tokens > HEAVY_TOKENS ? "excerpt" : "full" };
}

/** Raccourcit un ancien message très long (début et fin conservés). */
function shorten(text: string): string {
  if (text.length <= OLD_MESSAGE_MAX) return text;
  const head = text.slice(0, 1800);
  const tail = text.slice(-600);
  return `${head}\n[… ${text.length - 2400} caractères retirés de l'historique ancien pour économiser des jetons …]\n${tail}`;
}

/**
 * Historique envoyé au modèle. Une pièce jointe n'est renvoyée en entier que dans les derniers échanges, une image dans les deux
 * derniers, et les anciens messages très longs sont raccourcis. `tools` : le modèle sait lire la suite d'un extrait.
 */
export function wireHistory(messages: Msg[], tools: boolean) {
  const kept = messages.filter((m) => !m.skipWire);
  return kept.map((m, i) => {
    // Nombre de messages de l'utilisateur après celui-ci : son « âge ».
    const age = kept.slice(i + 1).filter((x) => x.role === "user").length;
    const files = (m.attachments ?? []).filter((a) => a.kind === "text");
    const images = (m.attachments ?? []).filter((a) => a.kind === "image");
    const old = kept.length - i > KEEP_FULL_MESSAGES;
    let content = old && m.role !== "system" ? shorten(m.content || m.notes?.at(-1)?.text || "") : m.content || m.notes?.at(-1)?.text || "";
    for (const f of files) {
      if (age >= KEEP_FILES_TURNS) {
        content += `\n\n[Fichier : ${f.name} — joint plus haut dans la conversation, contenu retiré de l'historique ancien pour économiser des jetons. Redemande-le à l'utilisateur si tu en as besoin.]`;
      } else if (f.mode === "excerpt" && tools) {
        content += "\n\n" + excerpt(f);
      } else {
        content += `\n\n[Fichier : ${f.name}]\n${FENCE}\n${f.data}\n${FENCE}`;
      }
    }
    const sendImages = age < KEEP_IMAGES_TURNS ? images.map((a) => a.data) : [];
    if (images.length && !sendImages.length) content += `\n\n[${images.length > 1 ? images.length + " images jointes" : "Image jointe"} plus haut dans la conversation, retirée${images.length > 1 ? "s" : ""} de l'historique ancien.]`;
    return { role: m.role, content, ...(sendImages.length ? { images: sendImages } : {}) };
  });
}

/** Fichiers texte de la conversation (dernier exemplaire de chaque nom) pour l'outil `read_attachment`. */
export function attachmentsForTool(messages: Msg[]): { name: string; content: string }[] {
  const byName = new Map<string, string>();
  for (const m of messages) for (const a of m.attachments ?? []) if (a.kind === "text") byName.set(a.name, a.data);
  return [...byName].map(([name, content]) => ({ name, content }));
}

/** Petits documents de projet envoyés en entier jusqu'à ce budget (caractères) ; les autres sont indexés. */
export const INLINE_DOC = 2500;
export const INLINE_BUDGET = 6000;

/** Sépare les documents d'un projet : ceux qui tiennent dans le contexte, et ceux que le modèle cherchera par passages. */
export function splitProjectDocs<T extends { content: string }>(docs: T[]): { inline: T[]; indexed: T[] } {
  const inline: T[] = [];
  const indexed: T[] = [];
  let budget = INLINE_BUDGET;
  for (const d of docs) {
    if (d.content.length <= INLINE_DOC && d.content.length <= budget) {
      inline.push(d);
      budget -= d.content.length;
    } else indexed.push(d);
  }
  return { inline, indexed };
}
