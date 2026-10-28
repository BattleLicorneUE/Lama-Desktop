<script lang="ts">
  import type { ChatStore } from "$lib/chat.svelte";
  import { SEVERITY_ORDER, excerpt, type Finding } from "$lib/review";

  let { chat, ongo }: { chat: ChatStore; ongo?: () => void } = $props();

  const r = $derived(chat.review);
  const sorted = $derived(r ? [...r.findings].sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity] || a.file.localeCompare(b.file)) : []);
  let open = $state<number | null>(null);

  const LABEL: Record<string, string> = { bug: "Bug", risque: "Risque", style: "Style", suggestion: "Suggestion" };
  const where = (f: Finding) => `${f.file}${f.line ? `:${f.line}` : ""}`;

  function fix(f: Finding) {
    chat.send(`Corrige ce problème relevé par la revue de code.\nFichier : ${where(f)}\nProblème : ${f.title}. ${f.detail}\nCorrection proposée : ${f.fix || "à toi de voir"}\nFais la modification minimale, puis vérifie avec verify.`);
  }

  function fixAll() {
    const list = sorted.filter((f) => f.severity === "bug" || f.severity === "risque");
    if (!list.length) return;
    chat.send(
      `Corrige ces problèmes relevés par la revue de code, un par un, puis vérifie avec verify :\n` +
        list.map((f, i) => `${i + 1}. ${where(f)} — ${f.title}. ${f.detail} (correction proposée : ${f.fix || "à toi de voir"})`).join("\n"),
    );
  }

  const important = $derived(sorted.filter((f) => f.severity === "bug" || f.severity === "risque").length);
</script>

<div class="rev">
  <div class="bar">
    <button class="primary" onclick={() => chat.reviewChanges()} disabled={chat.reviewing || chat.busy || !chat.workspace}>
      {chat.reviewing ? "Relecture en cours…" : r ? "Relire à nouveau" : "Relire les changements"}
    </button>
    {#if important && !chat.busy}<button onclick={fixAll} title="Demande à l'agent de corriger les bugs et les risques">Corriger les {important} importants</button>{/if}
  </div>

  {#if chat.reviewError}<p class="err" role="alert">{chat.reviewError}</p>{/if}

  {#if !r && !chat.reviewing && !chat.reviewError}
    <p class="hint">Le modèle relit tous les changements du projet (fichiers modifiés et nouveaux) et signale bugs, risques et problèmes de style, ligne par ligne. Tu peux aussi lui demander de les corriger.</p>
  {/if}

  {#if r}
    <p class="sum">{r.summary || "Revue terminée."} <span class="muted">· {r.files} fichier{r.files > 1 ? "s" : ""}</span></p>
    {#if !sorted.length}<p class="ok">Aucun problème relevé.</p>{/if}
    <ul>
      {#each sorted as f, i (i)}
        <li class={f.severity}>
          <button class="row" onclick={() => (open = open === i ? null : i)} aria-expanded={open === i}>
            <span class="tag">{LABEL[f.severity]}</span>
            <span class="title">{f.title}</span>
            <code class="loc">{where(f)}</code>
          </button>
          {#if open === i}
            {@const lines = excerpt(r.diff, f.file, f.line)}
            <div class="body">
              {#if f.detail}<p>{f.detail}</p>{/if}
              {#if lines.length}
                <pre class="code">{#each lines as l}<span class:add={l.kind === "add"} class:del={l.kind === "del"} class:hit={l.newN === f.line}>{(l.newN ?? "").toString().padStart(4)} {l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "} {l.text || " "}</span>{/each}</pre>
              {/if}
              {#if f.fix}<p class="fix"><strong>Correction proposée :</strong> {f.fix}</p>{/if}
              <button class="mini" onclick={() => fix(f)} disabled={chat.busy}>Demander la correction à l'agent</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .rev { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding: 8px; }
  .bar { display: flex; flex-wrap: wrap; gap: 6px; }
  .bar button, .mini { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 5px 11px; font-size: 13px; }
  .bar .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  button:disabled { opacity: 0.5; }
  .hint, .sum, .ok { margin: 0; font-size: 13px; line-height: 1.5; color: var(--muted); }
  .sum { color: var(--fg); }
  .ok { color: var(--add); }
  .muted { color: var(--muted); }
  .err { margin: 0; color: var(--del); font-size: 13px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { border: 1px solid var(--border); border-radius: 10px; background: var(--surface); overflow: hidden; }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; border: 0; background: none; padding: 7px 10px; text-align: left; font-size: 13px; color: var(--fg); }
  .tag { flex: none; font-size: 12px; font-weight: 700; padding: 1px 7px; border-radius: 6px; background: var(--hover); }
  li.bug .tag { background: color-mix(in srgb, var(--del) 20%, transparent); color: var(--del); }
  li.risque .tag { background: color-mix(in srgb, #d98b1f 22%, transparent); color: #b8740f; }
  li.style .tag, li.suggestion .tag { color: var(--muted); }
  .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .loc { flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--muted); background: var(--code); border-radius: 5px; padding: 1px 5px; }
  .body { border-top: 1px solid var(--border); padding: 8px 10px 10px; display: flex; flex-direction: column; gap: 8px; font-size: 13px; line-height: 1.5; }
  .body p { margin: 0; }
  .fix { color: var(--muted); }
  .code { margin: 0; padding: 6px 0; background: var(--code); border-radius: 8px; overflow-x: auto; font-family: ui-monospace, Consolas, monospace; font-size: 12px; line-height: 1.5; }
  .code span { display: block; padding: 0 8px; white-space: pre; }
  .code .add { background: var(--add-bg); color: var(--add); }
  .code .del { background: var(--del-bg); color: var(--del); }
  .code .hit { outline: 1px solid var(--accent); outline-offset: -1px; }
  .mini { align-self: flex-start; }
</style>
