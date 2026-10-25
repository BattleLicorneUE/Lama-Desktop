<script lang="ts">
  import type { ChatStore, ToolEvent } from "$lib/chat.svelte";
  import { toolLabel } from "$lib/tool-labels";

  let { tool }: { tool: ToolEvent; chat?: ChatStore } = $props();

  const STATES: Record<string, string> = {
    awaiting: "En attente", running: "En cours", done: "Terminé", error: "Erreur", denied: "Refusé", undone: "Annulé",
  };

  const base = $derived(toolLabel(tool.name));
  const label = $derived(tool.agent ? `${tool.agent} · ${base}` : base);
  const target = $derived(
    tool.name === "delegate" ? `${tool.args.agent} — ${String(tool.args.task ?? "").replace(/\s+/g, " ").slice(0, 90)}` : tool.name === "move_path" ? `${tool.args.from} → ${tool.args.to}` : String(tool.args.path ?? tool.args.command ?? tool.args.pattern ?? tool.args.target ?? tool.args.name ?? tool.args.query ?? tool.args.url ?? tool.args.id ?? ""),
  );
  const diffLines = $derived(tool.diff ? tool.diff.split("\n") : []);
  let open = $state(false);
  const ask = $derived(tool.state === "awaiting");
</script>

<div class="card" class:ask class:err={tool.state === "error" || tool.state === "denied"} class:undone={tool.state === "undone"}>
  <button class="head" onclick={() => (open = !open)} aria-expanded={open || ask}>
    <span class="lbl">{label}</span>
    <code class="target">{target}</code>
    <span class="state {tool.state}">{STATES[tool.state]}</span>
  </button>

  {#if ask || open || tool.state === "running"}
    <div class="body">
      {#if tool.name === "run_command"}
        <pre class="cmd">{tool.args.command}{tool.args.background ? "   (en arrière-plan)" : ""}</pre>
      {:else if tool.name === "delete_path"}
        <p class="note">Envoyé à la corbeille Windows (récupérable) : <code>{target}</code></p>
      {:else if tool.name === "make_dir" || tool.name === "move_path"}
        <p class="note"><code>{target}</code></p>
      {:else if tool.name.startsWith("mcp__")}
        <pre class="cmd">{JSON.stringify(tool.args, null, 2)}</pre>
      {/if}
      {#if diffLines.length}
        <pre class="diff">{#each diffLines as l}<span class:add={l.startsWith("+") && !l.startsWith("+++")} class:del={l.startsWith("-") && !l.startsWith("---")} class:hunk={l.startsWith("@@")}>{l}
</span>{/each}</pre>
      {/if}
      {#if tool.output && !ask}<pre class="out">{tool.output}</pre>{/if}
    </div>
  {/if}

  {#if ask}
    <p class="wait" role="status">Une fenêtre te demande si tu autorises cette action.</p>
  {/if}
</div>

<style>
  .card { border: 1px solid var(--border); border-radius: 12px; margin: 8px 0; background: var(--surface); overflow: hidden; font-family: var(--sans); font-size: 14px; }
  .card.ask { border-color: var(--accent); }
  .card.err { border-color: color-mix(in srgb, var(--del) 45%, var(--border)); }
  .head { width: 100%; display: flex; align-items: center; gap: 10px; border: 0; background: none; padding: 8px 12px; text-align: left; }
  .lbl { font-weight: 600; white-space: nowrap; }
  .target { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); font-size: 13px; }
  .state { font-size: 12px; padding: 1px 8px; border-radius: 6px; background: var(--hover); color: var(--muted); white-space: nowrap; }
  .state.done { color: var(--add); }
  .card.undone { opacity: 0.6; }
  .state.error, .state.denied { color: var(--del); }
  .state.awaiting { color: var(--accent); }
  .body { border-top: 1px solid var(--border); }
  pre { margin: 0; padding: 10px 12px; overflow-x: auto; max-height: 340px; overflow-y: auto; font-family: ui-monospace, Consolas, monospace; font-size: 12.5px; line-height: 1.5; }
  .cmd { background: var(--code); }
  .note { margin: 0; padding: 10px 12px; color: var(--muted); font-size: 13px; overflow-wrap: anywhere; }
  .out { color: var(--muted); white-space: pre-wrap; }
  .diff span { display: block; }
  .diff .add { background: var(--add-bg); color: var(--add); }
  .diff .del { background: var(--del-bg); color: var(--del); }
  .diff .hunk { color: var(--muted); }
  .wait { margin: 0; padding: 6px 12px 8px; color: var(--accent); font-size: 13px; }
</style>
