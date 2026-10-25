/** Export d'une conversation en Markdown lisible ou en JSON. */
export type StoredMessage = { role: string; content: string; thinking?: string | null; extra?: string | null };
export type ExportMeta = { title: string; model: string; workspace: string | null; date: Date };

type Extra = { tools?: { name: string; args: Record<string, unknown>; state: string }[]; tasks?: { content: string; status: string }[] };

const parseExtra = (s?: string | null): Extra => {
  try {
    return s ? (JSON.parse(s) as Extra) : {};
  } catch {
    return {};
  }
};

const ROLES: Record<string, string> = { user: "Toi", assistant: "Assistant", system: "Résumé de la conversation" };

/** Nom de fichier sûr (sans caractères interdits sous Windows). */
export function fileSlug(title: string): string {
  const s = title.replace(/[\\/:*?"<>|\u0000-\u001f]/g, " ").replace(/\s+/g, " ").trim().slice(0, 60).trim();
  return s || "conversation";
}

/** Clôture de code assez longue pour ne pas être fermée par le contenu. */
const fence = (text: string) => "`".repeat(Math.max(3, ...(text.match(/`+/g) ?? []).map((m) => m.length + 1)));

function target(args: Record<string, unknown>): string {
  return String(args.path ?? args.command ?? args.pattern ?? args.query ?? args.url ?? args.name ?? args.from ?? "").replace(/\s+/g, " ").slice(0, 120);
}

/** Les messages en Markdown ; `level` : niveau des titres « Toi » / « Assistant » (2 = `##`). */
export function transcript(messages: StoredMessage[], level = 2): string {
  const lines: string[] = [];
  const h = "#".repeat(level);
  for (const m of messages) {
    lines.push(`${h} ${ROLES[m.role] ?? m.role}`, "");
    if (m.thinking?.trim()) {
      const f = fence(m.thinking);
      lines.push("<details><summary>Réflexion</summary>", "", f, m.thinking.trim(), f, "", "</details>", "");
    }
    const extra = parseExtra(m.extra);
    if (extra.tasks?.length) {
      lines.push(...extra.tasks.map((t) => `- [${t.status === "completed" ? "x" : " "}] ${t.content}`), "");
    }
    if (extra.tools?.length) {
      lines.push(...extra.tools.map((t) => `- \`${t.name}\` ${target(t.args)} *(${t.state})*`), "");
    }
    if (m.content.trim()) lines.push(m.content.trim(), "");
  }
  return lines.join("\n");
}

/** Texte propre pour un document (PDF, Word, HTML) : sans réflexion ni appels d'outils. */
export function toDocMarkdown(messages: StoredMessage[]): string {
  const parts: string[] = [];
  for (const m of messages) {
    if (!m.content.trim()) continue;
    parts.push(`## ${ROLES[m.role] ?? m.role}`, "", m.content.trim(), "");
  }
  return parts.join("\n");
}

export function toMarkdown(messages: StoredMessage[], meta: ExportMeta): string {
  const head = [`# ${meta.title}`, "", `*${meta.model} · ${meta.date.toLocaleString("fr-FR")}${meta.workspace ? ` · ${meta.workspace}` : ""}*`, ""];
  return head.join("\n") + "\n" + transcript(messages);
}

export function toJson(messages: StoredMessage[], meta: ExportMeta): string {
  return JSON.stringify(
    {
      title: meta.title,
      model: meta.model,
      workspace: meta.workspace,
      exported_at: meta.date.toISOString(),
      messages: messages.map((m) => ({ role: m.role, content: m.content, thinking: m.thinking ?? null, ...parseExtra(m.extra) })),
    },
    null,
    2,
  );
}
