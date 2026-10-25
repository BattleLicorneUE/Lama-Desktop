<script lang="ts">
  import type { ChatStore, Project } from "$lib/chat.svelte";
  import { projectBlurb, shortDate } from "$lib/projects";
  import Icon from "$lib/Icon.svelte";

  let { chat }: { chat: ChatStore } = $props();
  let query = $state("");
  let searching = $state(false);
  let byName = $state(false);

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = chat.modeProjects.filter((p) => !q || p.name.toLowerCase().includes(q) || p.instructions.toLowerCase().includes(q));
    return list.toSorted((a, b) => Number(b.pinned) - Number(a.pinned) || (byName ? a.name.localeCompare(b.name, "fr") : b.updated_at - a.updated_at));
  });

  function focusNode(node: HTMLInputElement) {
    node.focus();
  }

  function pin(e: Event, p: Project) {
    e.stopPropagation();
    void chat.pinProject(p);
  }
</script>

<div class="page">
  <header>
    <h1>Projets</h1>
    <div class="tools">
      {#if searching}
        <input class="q" use:focusNode bind:value={query} placeholder="Rechercher un projet…" aria-label="Rechercher un projet" onkeydown={(e) => { if (e.key === "Escape") { searching = false; query = ""; } }} />
      {/if}
      <button class="icon" onclick={() => { searching = !searching; if (!searching) query = ""; }} title="Rechercher" aria-label="Rechercher un projet" aria-pressed={searching}><Icon name="search" size={18} /></button>
      <button class="icon" onclick={() => (byName = !byName)} title={byName ? "Trié par nom : trier par activité récente" : "Trié par activité récente : trier par nom"} aria-label="Changer le tri"><Icon name="sort" size={18} /></button>
      <button class="new" onclick={() => (chat.projectDialog = "new")}>Nouveau projet</button>
    </div>
  </header>

  {#if shown.length}
    <ul class="grid">
      {#each shown as p (p.id)}
        <li>
          <div class="card" role="button" tabindex="0" onclick={() => chat.openProject(p.id)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); chat.openProject(p.id); } }}>
            <div class="top">
              <strong>{p.name}</strong>
              <button class="pin" class:on={p.pinned} onclick={(e) => pin(e, p)} title={p.pinned ? "Désépingler" : "Épingler en haut"} aria-label={p.pinned ? `Désépingler ${p.name}` : `Épingler ${p.name}`} aria-pressed={p.pinned}><Icon name="pin" size={14} /></button>
            </div>
            <p class="blurb">{projectBlurb(p.instructions)}</p>
            <div class="foot"><span>{shortDate(p.updated_at)}</span>{#if p.chats}<span>· {p.chats} {chat.mode === "code" ? (p.chats > 1 ? "sessions" : "session") : p.chats > 1 ? "chats" : "chat"}</span>{/if}{#if p.workspace}<span class="dir" title={p.workspace}>· {p.workspace.split(/[\\/]/).filter(Boolean).pop()}</span>{/if}</div>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="none">{chat.modeProjects.length ? "Aucun projet ne correspond à ta recherche." : chat.mode === "code" ? "Aucun projet de code pour l'instant. Un projet relie un dossier de travail à des instructions et à l'historique de ses sessions." : "Aucun projet pour l'instant. Un projet regroupe des chats autour d'instructions et de documents communs."}</p>
  {/if}
</div>

<style>
  .page { flex: 1; overflow: auto; padding: 56px max(24px, calc((100% - 960px) / 2)) 40px; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin-bottom: 28px; }
  h1 { margin: 0; font-size: 30px; font-weight: 500; font-family: Georgia, "Times New Roman", serif; }
  .tools { display: flex; align-items: center; gap: 8px; }
  .icon { display: grid; place-items: center; border: 0; background: none; color: var(--fg); width: 34px; height: 34px; border-radius: 8px; }
  .icon:hover, .icon[aria-pressed="true"] { background: var(--hover); }
  .q { width: 220px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 10px; font-size: 14px; color: var(--fg); outline: none; }
  .q:focus { border-color: var(--accent); }
  .new { border: 0; background: var(--fg); color: var(--bg); border-radius: 8px; padding: 8px 14px; font-size: 14px; font-weight: 500; }
  .grid { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 380px), 1fr)); gap: 14px; }
  .card { height: 100%; min-height: 120px; box-sizing: border-box; display: flex; flex-direction: column; gap: 6px; border: 1px solid var(--border); border-radius: 12px; padding: 16px 18px; background: var(--surface); cursor: pointer; }
  .card { transition: transform 0.18s ease, box-shadow 0.18s ease, border-color 0.15s; }
  .card:hover { transform: translateY(-3px); box-shadow: 0 10px 24px rgb(0 0 0 / 0.14); }
  .card:hover, .card:focus-visible { border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); outline: none; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  strong { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pin { border: 0; background: none; color: var(--muted); padding: 3px; border-radius: 6px; opacity: 0; display: grid; }
  .card:hover .pin, .pin.on, .pin:focus-visible { opacity: 1; }
  .pin.on { color: var(--accent); }
  .pin:hover { background: var(--hover); }
  .blurb { margin: 0; color: var(--muted); font-size: 14px; line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; flex: 1; }
  .foot { display: flex; gap: 6px; color: var(--muted); font-size: 13px; margin-top: 8px; }
  .dir { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .none { color: var(--muted); max-width: 520px; line-height: 1.5; }
</style>
