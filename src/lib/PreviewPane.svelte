<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { EXT, LANG_LABEL, buildDoc, normLang, type Artifact } from "$lib/artifacts";
  import { renderMarkdown } from "$lib/markdown";

  let { code, lang, versions = [], onselect, onclose }: {
    code: string;
    lang: string;
    versions?: Artifact[];
    onselect?: (a: Artifact) => void;
    onclose: () => void;
  } = $props();

  const kind = $derived(normLang(lang));
  // Le code affiché peut être modifié à la main : « Appliquer » met l'aperçu à jour sans repasser par le modèle.
  let edited = $state("");
  let live = $state("");
  $effect(() => {
    edited = live = code;
  });
  const changed = $derived(live !== code);

  const dark = $derived.by(() => {
    const t = document.documentElement.dataset.theme;
    return t ? t === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
  });
  const doc = $derived(kind === "markdown" ? "" : buildDoc(kind, live, dark));

  const index = $derived(versions.findLastIndex((v) => v.code === code && normLang(v.lang) === kind));
  const label = $derived(LANG_LABEL[kind] ?? kind);

  let tab = $state<"preview" | "code">("preview");
  let note = $state("");
  let runs = $state(0);

  const flash = (t: string) => {
    note = t;
    setTimeout(() => (note = ""), 2200);
  };

  function go(delta: number) {
    const v = versions[index + delta];
    if (v) onselect?.(v);
  }

  function apply() {
    live = edited;
    tab = "preview";
    runs++;
  }

  async function copy() {
    await navigator.clipboard.writeText(live);
    flash("Copié");
  }

  /** SVG -> PNG : l'image est dessinée sur un canevas (largeur 1600 px) puis enregistrée. */
  async function savePng() {
    try {
      let svg = live.trim();
      if (!/<svg[^>]*xmlns=/.test(svg)) svg = svg.replace("<svg", '<svg xmlns="http://www.w3.org/2000/svg"');
      const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
      const image = new Image();
      await new Promise<void>((ok, fail) => {
        image.onload = () => ok();
        image.onerror = () => fail(new Error("SVG illisible"));
        image.src = url;
      });
      const ratio = image.naturalWidth && image.naturalHeight ? image.naturalHeight / image.naturalWidth : 1;
      const canvas = document.createElement("canvas");
      canvas.width = 1600;
      canvas.height = Math.max(1, Math.round(1600 * ratio));
      canvas.getContext("2d")!.drawImage(image, 0, 0, canvas.width, canvas.height);
      URL.revokeObjectURL(url);
      const path = await saveDialog({ defaultPath: "image.png", filters: [{ name: "PNG", extensions: ["png"] }] });
      if (!path) return;
      await invoke("save_bytes", { path, base64: canvas.toDataURL("image/png").split(",")[1] });
      flash("PNG enregistré");
    } catch (e) {
      flash(String(e instanceof Error ? e.message : e));
    }
  }

  async function save() {
    const ext = EXT[kind] ?? "txt";
    const path = await saveDialog({ defaultPath: `apercu.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    if (!path) return;
    try {
      await invoke("save_text", { path, content: live });
      flash("Enregistré");
    } catch (e) {
      flash(String(e));
    }
  }

  /** Page HTML : PDF fidèle à l'affichage (sans les scripts). */
  async function exportHtmlPdf() {
    const title = /<title>([^<]{1,80})<\/title>/i.exec(live)?.[1]?.trim() ?? "Page";
    const path = await saveDialog({ defaultPath: `${title.replace(/[\\/:*?"<>|]/g, " ")}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (!path) return;
    try {
      await invoke("export_html_pdf", { path, title, html: live });
      flash("PDF enregistré");
    } catch (e) {
      flash(String(e));
    }
  }

  /** Document Markdown : export en PDF, Word, page web. */
  async function exportDoc() {
    const title = /^#\s+(.+)$/m.exec(live)?.[1]?.trim() ?? "Document";
    const path = await saveDialog({
      defaultPath: `${title.replace(/[\\/:*?"<>|]/g, " ").slice(0, 60)}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }, { name: "Word", extensions: ["docx"] }, { name: "Page web", extensions: ["html"] }, { name: "Markdown", extensions: ["md"] }],
    });
    if (!path) return;
    try {
      await invoke("export_document", { path, title, markdown: live });
      flash("Exporté");
    } catch (e) {
      flash(String(e));
    }
  }
</script>

<aside>
  <div class="head">
    <div class="tabs">
      <button class:on={tab === "preview"} onclick={() => (tab = "preview")}>Aperçu</button>
      <button class:on={tab === "code"} onclick={() => (tab = "code")}>Code{#if changed} •{/if}</button>
    </div>
    {#if versions.length > 1 && index >= 0}
      <div class="ver" role="group" aria-label="Versions">
        <button onclick={() => go(-1)} disabled={index <= 0} aria-label="Version précédente">‹</button>
        <span title={label}>{index + 1} / {versions.length}</span>
        <button onclick={() => go(1)} disabled={index >= versions.length - 1} aria-label="Version suivante">›</button>
      </div>
    {/if}
    <div class="tools">
      {#if note}<span class="note" role="status">{note}</span>{/if}
      {#if tab === "preview" && kind !== "markdown"}<button onclick={() => runs++} title="Relancer l'aperçu">Relancer</button>{/if}
      <button onclick={copy}>Copier</button>
      {#if kind === "svg"}<button onclick={savePng} title="Exporter l'image en PNG">PNG</button>{/if}
      {#if kind === "html"}<button onclick={exportHtmlPdf} title="PDF de la page, sans les scripts">PDF</button>{/if}
      {#if kind === "markdown"}<button onclick={exportDoc} title="Exporter en PDF, Word, page web">Exporter</button>{/if}
      <button onclick={save}>Enregistrer</button>
      <button class="x" onclick={onclose} aria-label="Fermer l'aperçu">×</button>
    </div>
  </div>
  {#if tab === "preview"}
    {#if kind === "markdown"}
      <div class="docview"><div class="md">{@html renderMarkdown(live)}</div></div>
    {:else}
      {#key runs + live}<iframe title="Aperçu" sandbox="allow-scripts" srcdoc={doc}></iframe>{/key}
    {/if}
  {:else}
    <textarea bind:value={edited} spellcheck="false" aria-label="Code de l'aperçu"></textarea>
    <div class="editbar">
      <span class="muted">Modifie le code, puis applique pour voir le résultat.</span>
      <button onclick={() => (edited = code)} disabled={edited === code && live === code}>Rétablir</button>
      <button class="primary" onclick={apply} disabled={edited === live}>Appliquer</button>
    </div>
  {/if}
</aside>

<style>
  aside { width: 46%; min-width: 320px; border-left: 1px solid var(--border); display: flex; flex-direction: column; background: var(--bg); }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .tabs, .tools, .ver { display: flex; gap: 4px; align-items: center; }
  .ver { color: var(--muted); font-size: 13px; font-variant-numeric: tabular-nums; }
  .ver button:disabled { opacity: 0.35; }
  .tabs button, .tools button, .ver button { border: 0; background: none; color: var(--muted); padding: 5px 10px; border-radius: 8px; font-size: 13px; }
  .tabs button.on { background: var(--hover); color: var(--fg); }
  .tools button:hover, .ver button:hover:not(:disabled) { background: var(--hover); color: var(--fg); }
  .note { color: var(--accent); font-size: 12.5px; }
  .x { font-size: 22px !important; line-height: 1; }
  iframe { flex: 1; border: 0; background: white; }
  .docview { flex: 1; overflow: auto; padding: 22px 26px; }
  textarea { flex: 1; margin: 0; padding: 14px; border: 0; outline: none; resize: none; font-family: ui-monospace, Consolas, monospace; font-size: 13px; line-height: 1.5; background: var(--code); color: var(--fg); }
  .editbar { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-top: 1px solid var(--border); }
  .editbar .muted { flex: 1; color: var(--muted); font-size: 12.5px; }
  .editbar button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 12px; font-size: 13px; }
  .editbar .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .editbar button:disabled { opacity: 0.45; }
</style>
