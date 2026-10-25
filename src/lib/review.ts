/** Revue de code : le modèle relit le diff et rend des remarques rattachées à des lignes. */

export type DiffLine = { kind: "add" | "del" | "ctx" | "hunk"; text: string; oldN: number | null; newN: number | null };
export type DiffFile = { file: string; lines: DiffLine[] };

export type Severity = "bug" | "risque" | "style" | "suggestion";
export type Finding = { file: string; line: number | null; severity: Severity; title: string; detail: string; fix: string };
export type ReviewResult = { summary: string; findings: Finding[]; at: number; files: number; diff: DiffFile[] };

/** Découpe un diff unifié en fichiers et en lignes numérotées (numéros de l'ancien et du nouveau fichier). */
export function parseDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  let cur: DiffFile | null = null;
  let o = 0;
  let n = 0;
  for (const raw of diff.split("\n")) {
    const head = /^diff --git a\/(.+?) b\/(.+)$/.exec(raw);
    if (head) {
      cur = { file: head[2], lines: [] };
      files.push(cur);
      continue;
    }
    if (!cur) continue;
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
    if (hunk) {
      o = Number(hunk[1]);
      n = Number(hunk[2]);
      cur.lines.push({ kind: "hunk", text: raw, oldN: null, newN: null });
      continue;
    }
    if (raw.startsWith("+++") || raw.startsWith("---") || raw.startsWith("index ") || raw.startsWith("new file") || raw.startsWith("deleted file") || raw.startsWith("similarity") || raw.startsWith("rename ") || raw.startsWith("\\")) continue;
    if (raw.startsWith("+")) cur.lines.push({ kind: "add", text: raw.slice(1), oldN: null, newN: n++ });
    else if (raw.startsWith("-")) cur.lines.push({ kind: "del", text: raw.slice(1), oldN: o++, newN: null });
    else if (raw.startsWith(" ")) cur.lines.push({ kind: "ctx", text: raw.slice(1), oldN: o++, newN: n++ });
  }
  return files;
}

/** Le diff écrit pour le modèle : chaque ligne porte son numéro dans le nouveau fichier, qu'il peut citer tel quel. */
export function annotate(files: DiffFile[]): string {
  const out: string[] = [];
  for (const f of files) {
    out.push(`=== ${f.file} ===`);
    for (const l of f.lines) {
      if (l.kind === "hunk") out.push(l.text);
      else if (l.kind === "del") out.push(`     - ${l.text}`);
      else out.push(`${String(l.newN).padStart(5)} ${l.kind === "add" ? "+" : " "} ${l.text}`);
    }
  }
  return out.join("\n");
}

export const REVIEW_PROMPT = `Tu es un relecteur de code senior, exigeant mais juste. On te donne les changements d'un projet : pour chaque fichier, les lignes du NOUVEAU fichier avec leur numéro (« + » = ligne ajoutée ou modifiée, « - » = ligne supprimée, sans numéro).
Relis ces changements et signale uniquement de VRAIS problèmes qu'une personne de l'équipe voudrait corriger avant de valider :
- « bug » : erreur de logique, cas limite oublié, plantage, fuite, régression probable ;
- « risque » : sécurité, perte de données, concurrence, gestion d'erreur absente, secret en clair ;
- « style » : lisibilité, nommage, duplication, code mort, commentaire trompeur ;
- « suggestion » : amélioration utile mais facultative.
Règles : pas de remarque sur une ligne que tu n'as pas vue ; pas de reproche vague ; au plus 12 remarques, les plus importantes d'abord ; si tout est correct, renvoie une liste vide.
Réponds UNIQUEMENT par un objet JSON, sans texte autour, de cette forme :
{"summary":"une ou deux phrases sur l'ensemble des changements","findings":[{"file":"chemin/du/fichier","line":42,"severity":"bug","title":"titre court","detail":"explication précise du problème et de son effet","fix":"correction proposée, en une ou deux phrases"}]}
« line » est le numéro de la ligne concernée dans le nouveau fichier (nombre), ou null si la remarque concerne tout le fichier.`;

const SEVERITIES: Severity[] = ["bug", "risque", "style", "suggestion"];

/** Lit la réponse du modèle, même entourée de texte ou de ```json. */
export function parseReview(output: string, files: DiffFile[]): { summary: string; findings: Finding[] } {
  const text = output.replace(/```(?:json)?/gi, "");
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) throw new Error("Le modèle n'a pas rendu de revue exploitable : relance-la.");
  let data: { summary?: unknown; findings?: unknown };
  try {
    data = JSON.parse(text.slice(start, end + 1));
  } catch {
    throw new Error("La réponse du modèle n'est pas un JSON valide : relance la revue.");
  }
  const known = new Set(files.map((f) => f.file));
  const list = Array.isArray(data.findings) ? data.findings : [];
  const findings: Finding[] = list
    .map((f: Record<string, unknown>) => ({
      file: String(f.file ?? "").replace(/^[ab]\//, ""),
      line: Number.isFinite(Number(f.line)) && f.line !== null && f.line !== "" ? Number(f.line) : null,
      severity: (SEVERITIES.includes(f.severity as Severity) ? f.severity : "suggestion") as Severity,
      title: String(f.title ?? "").trim(),
      detail: String(f.detail ?? "").trim(),
      fix: String(f.fix ?? "").trim(),
    }))
    // Une remarque sur un fichier qui n'est pas dans le diff est une invention du modèle.
    .filter((f) => f.title && known.has(f.file))
    .slice(0, 20);
  return { summary: String(data.summary ?? "").trim(), findings };
}

/** Quelques lignes du diff autour d'une remarque, pour la voir en contexte. */
export function excerpt(files: DiffFile[], file: string, line: number | null, radius = 3): DiffLine[] {
  const f = files.find((x) => x.file === file);
  if (!f) return [];
  if (line === null) return f.lines.filter((l) => l.kind !== "hunk").slice(0, radius * 2);
  const i = f.lines.findIndex((l) => l.newN === line);
  if (i < 0) return [];
  return f.lines.slice(Math.max(0, i - radius), i + radius + 1).filter((l) => l.kind !== "hunk");
}

export const SEVERITY_ORDER: Record<Severity, number> = { bug: 0, risque: 1, style: 2, suggestion: 3 };
