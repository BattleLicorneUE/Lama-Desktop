/** `/memoire` : condenser une conversation en quelques lignes de mémoire durable. */

export const MEMORY_PROMPT = `Tu condenses une conversation en mémoire durable pour un assistant, qui la relira dans de futures conversations. Réponds en français, exactement dans ce format, sans introduction :

TITRE: un titre très court (50 caractères maximum)
- trois à six puces, une ligne chacune : ce qu'il faut retenir (faits sur l'utilisateur, décisions, informations utiles, état d'avancement, préférences). Pas de code long, pas d'anecdotes.`;

const MAX_POINT = 160;
const MAX_TOTAL = 600;

const clip = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1).trimEnd() + "…" : s);

/** Titre et points retenus, prêts à tenir sur une ligne de mémoire. */
export function parseMemory(output: string | null): { title: string; points: string } {
  const lines = (output ?? "").split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  const titleLine = lines.find((l) => /^titre\s*:/i.test(l));
  const title = (titleLine ?? "").replace(/^titre\s*:/i, "").replace(/^["«»#\s]+|["«»\s]+$/g, "");
  const bullets = lines.filter((l) => /^[-*•]\s+/.test(l)).map((l) => l.replace(/^[-*•]\s+/, "").replace(/\s+/g, " "));
  const body = bullets.length ? bullets : lines.filter((l) => l !== titleLine).map((l) => l.replace(/\s+/g, " "));
  const points = clip(body.map((b) => clip(b, MAX_POINT)).join(" ; "), MAX_TOTAL);
  return { title: clip(title, 60), points };
}

/** Phrase ajoutée à la mémoire. */
export function memoryLine(title: string, date: string, points: string): string {
  return `Conversation « ${title || "sans titre"} » du ${date} : ${points}`;
}
