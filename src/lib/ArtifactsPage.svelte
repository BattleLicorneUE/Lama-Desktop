<script lang="ts">
  import type { ChatStore, SavedArtifact } from "$lib/chat.svelte";
  import { CREATIONS, KIND_LABEL, buildDoc, kindOf, normLang, type CreationKind } from "$lib/artifacts";
  import { askConfirm } from "$lib/dialogs.svelte";
  import { renderMarkdown } from "$lib/markdown";
  import { shortDate } from "$lib/projects";
  import Icon from "$lib/Icon.svelte";

  let { chat }: { chat: ChatStore } = $props();

  const FILTERS = [
    { id: "all", label: "Tous" },
    { id: "document", label: "Documents" },
    { id: "slides", label: "Diapositives" },
    { id: "design", label: "Design" },
    { id: "dashboard", label: "Tableaux de bord" },
    { id: "other", label: "Autres" },
  ];
  let filter = $state("all");
  let query = $state("");
  let searching = $state(false);
  let byName = $state(false);

  const group = (a: SavedArtifact) => {
    const k = kindOf(a.kind, a.lang);
    return ["document", "slides", "design", "dashboard"].includes(k) ? k : "other";
  };
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return chat.artifacts
      .filter((a) => (filter === "all" || group(a) === filter) && (!q || a.title.toLowerCase().includes(q)))
      .toSorted((a, b) => (byName ? a.title.localeCompare(b.title, "fr") : b.updated_at - a.updated_at));
  });

  const TILES: { kind: CreationKind; art: string }[] = [
    { kind: "document", art: "doc" },
    { kind: "slides", art: "slides" },
    { kind: "design", art: "design" },
    { kind: "dashboard", art: "dash" },
  ];

  function focusNode(node: HTMLInputElement) {
    node.focus();
  }

  async function remove(e: Event, a: SavedArtifact) {
    e.stopPropagation();
    if (await askConfirm(`Retirer « ${a.title} » de la bibliothèque ? La conversation d'origine est conservée.`)) await chat.deleteArtifact(a);
  }

  const live = (a: SavedArtifact) => ["html", "svg"].includes(normLang(a.lang));
</script>

<div class="page">
  <header>
    <h1>Artéfacts</h1>
  </header>

  <div class="bar">
    <div class="tabs" role="tablist">
      {#each FILTERS as f}
        <button role="tab" aria-selected={filter === f.id} class:on={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
      {/each}
    </div>
    <div class="tools">
      {#if searching}
        <input class="q" use:focusNode bind:value={query} placeholder="Rechercher…" aria-label="Rechercher un artéfact" onkeydown={(e) => { if (e.key === "Escape") { searching = false; query = ""; } }} />
      {/if}
      <button class="icon" onclick={() => { searching = !searching; if (!searching) query = ""; }} title="Rechercher" aria-label="Rechercher un artéfact" aria-pressed={searching}><Icon name="search" size={18} /></button>
      <button class="icon" onclick={() => (byName = !byName)} title={byName ? "Trié par nom : trier par date" : "Trié par date : trier par nom"} aria-label="Changer le tri"><Icon name="sort" size={18} /></button>
    </div>
  </div>

  <h2>Créer quelque chose de nouveau</h2>
  <ul class="tiles">
    {#each TILES as t}
      <li>
        <button class="tile" onclick={() => chat.startCreation(t.kind)}>
          <span class="art a-{t.art}" aria-hidden="true">
            {#if t.art === "doc"}
              <i class="pg"><b></b><u></u><u></u><u class="s"></u></i>
            {:else if t.art === "slides"}
              <i class="sl"><b></b><u></u><u class="s"></u><em></em></i>
            {:else if t.art === "design"}
              <i class="ph"><b></b><u></u><u></u></i>
            {:else}
              <i class="ch"><b style="height:30%"></b><b style="height:55%"></b><b style="height:85%"></b><b style="height:45%"></b></i>
            {/if}
          </span>
          <span class="lbl">{CREATIONS[t.kind].label}</span>
        </button>
      </li>
    {/each}
  </ul>

  {#if shown.length}
    <ul class="grid">
      {#each shown as a (a.id)}
        <li>
          <div class="card" role="button" tabindex="0" onclick={() => chat.openSavedArtifact(a)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); chat.openSavedArtifact(a); } }}>
            <div class="thumb" class:paper={normLang(a.lang) === "markdown"}>
              {#if normLang(a.lang) === "markdown"}
                <div class="md">{@html renderMarkdown(a.code.slice(0, 1400))}</div>
              {:else if live(a)}
                <iframe title={a.title} sandbox="allow-scripts" tabindex="-1" srcdoc={buildDoc(a.lang, a.code, false)} loading="lazy"></iframe>
              {:else}
                <span class="ph-ico"><Icon name="file" size={34} /></span>
              {/if}
            </div>
            <span class="badge" title={KIND_LABEL[kindOf(a.kind, a.lang)]}><Icon name="file" size={15} /></span>
            <button class="del" onclick={(e) => remove(e, a)} title="Retirer de la bibliothèque" aria-label="Retirer {a.title} de la bibliothèque"><Icon name="trash" size={14} /></button>
            <div class="info">
              <strong title={a.title}>{a.title}</strong>
              <span class="meta">{KIND_LABEL[kindOf(a.kind, a.lang)]} · Modifié {shortDate(a.updated_at)}{#if a.versions > 1} · {a.versions} versions{/if}</span>
            </div>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="none">{chat.artifacts.length ? "Aucun artéfact ne correspond." : "Rien ici pour l'instant. Les pages, documents et images créés dans tes chats apparaîtront ici ; ou lance une création ci-dessus."}</p>
  {/if}
</div>

<style>
  .page { flex: 1; overflow: auto; padding: 52px max(24px, calc((100% - 1000px) / 2)) 40px; }
  h1 { margin: 0 0 22px; font-size: 30px; font-weight: 500; font-family: Georgia, "Times New Roman", serif; }
  h2 { margin: 26px 0 12px; font-size: 14px; font-weight: 400; color: var(--muted); }
  .bar { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; }
  .tabs { display: flex; gap: 2px; flex-wrap: wrap; }
  .tabs button { border: 0; background: none; color: var(--muted); font-size: 14.5px; padding: 6px 11px; border-radius: 8px; }
  .tabs button:hover { color: var(--fg); }
  .tabs button.on { background: var(--hover); color: var(--fg); font-weight: 500; }
  .tools { display: flex; align-items: center; gap: 4px; }
  .icon { display: grid; place-items: center; border: 0; background: none; color: var(--fg); width: 34px; height: 34px; border-radius: 8px; }
  .icon:hover, .icon[aria-pressed="true"] { background: var(--hover); }
  .q { width: 200px; border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 10px; font-size: 14px; color: var(--fg); outline: none; }
  .q:focus { border-color: var(--accent); }

  .tiles { list-style: none; margin: 0 0 30px; padding: 0; display: flex; gap: 14px; flex-wrap: wrap; }
  .tile { display: flex; flex-direction: column; gap: 8px; border: 0; background: none; color: var(--fg); padding: 0; text-align: left; width: 168px; }
  .art { display: grid; place-items: center; height: 100px; border-radius: 12px; border: 1px solid var(--border); overflow: hidden; }
  .tile:hover .art { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); }
  .lbl { font-size: 14px; font-weight: 600; padding-left: 2px; }
  .a-doc { background: linear-gradient(#1c2a3f, #151d2b); }
  .a-slides { background: linear-gradient(#3b2d19, #2a2012); }
  .a-design { background: linear-gradient(#2b2a33, #1f1e26); }
  .a-dash { background: linear-gradient(#2f2850, #231e3d); }
  .art i { display: block; font-style: normal; position: relative; }
  .art b, .art u, .art em { display: block; border-radius: 3px; text-decoration: none; }
  .art .pg { width: 54px; height: 72px; border: 1px solid #4b6180; border-radius: 6px; background: #1d2a3d; padding: 8px 7px; display: grid; gap: 5px; align-content: start; }
  .pg b { height: 6px; width: 60%; background: #6f9be6; }
  .pg u { height: 3px; background: #53698a; } .pg u.s { width: 55%; }
  .art .sl { width: 96px; height: 62px; background: #4a3a21; border: 1px solid #7a6238; border-radius: 6px; padding: 9px; display: grid; gap: 6px; align-content: start; }
  .sl b { height: 7px; width: 55%; background: #e0a85c; } .sl u { height: 4px; background: #8b7449; } .sl u.s { width: 60%; }
  .sl em { position: absolute; right: 8px; bottom: 8px; width: 24px; height: 20px; background: #b88a4a; border-radius: 4px; }
  .art .ph { width: 44px; height: 76px; border: 2px solid #5a5870; border-radius: 9px; background: #2c2b3a; padding: 12px 7px; display: grid; gap: 7px; align-content: start; }
  .ph b { height: 9px; background: #8a82d8; } .ph u { height: 5px; background: #5b5972; }
  .art .ch { display: flex; gap: 8px; align-items: flex-end; height: 56px; width: 100px; padding: 10px 12px; background: #392f63; border: 1px solid #5b4fa0; border-radius: 8px; box-sizing: content-box; }
  .ch b { flex: 1; background: #8b7be0; border-radius: 3px 3px 0 0; }

  .grid { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, 304px); gap: 18px; }
  .card { position: relative; width: 304px; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--surface); cursor: pointer; }
  .card { transition: transform 0.18s ease, box-shadow 0.18s ease, border-color 0.15s; }
  .card:hover { transform: translateY(-3px); box-shadow: 0 10px 24px rgb(0 0 0 / 0.14); }
  .card:hover, .card:focus-visible { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); outline: none; }
  .thumb { position: relative; width: 100%; height: 180px; overflow: hidden; background: var(--hover); }
  .thumb.paper { background: #fff; color: #222; }
  .thumb iframe { width: 1216px; height: 720px; border: 0; background: #fff; transform: scale(0.25); transform-origin: 0 0; pointer-events: none; }
  .thumb .md { padding: 22px 26px; font-size: 12px; line-height: 1.5; pointer-events: none; }
  .thumb .md :global(h1) { font-size: 20px; font-weight: 500; font-family: Georgia, "Times New Roman", serif; margin: 0 0 8px; }
  .thumb .md :global(h2), .thumb .md :global(h3) { font-size: 14px; margin: 10px 0 4px; }
  .thumb .md :global(p), .thumb .md :global(li) { margin: 0 0 4px; }
  .ph-ico { position: absolute; inset: 0; display: grid; place-items: center; color: var(--muted); }
  .badge { position: absolute; left: 12px; top: 156px; display: grid; place-items: center; width: 30px; height: 30px; border-radius: 8px; background: var(--bg); border: 1px solid var(--border); color: var(--accent); }
  .del { position: absolute; top: 8px; right: 8px; display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 8px; background: color-mix(in srgb, var(--bg) 85%, transparent); color: var(--fg); opacity: 0; }
  .card:hover .del, .del:focus-visible { opacity: 1; }
  .info { padding: 22px 14px 14px; display: grid; gap: 4px; }
  .info strong { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--muted); font-size: 13px; }
  .none { color: var(--muted); max-width: 520px; line-height: 1.5; }
</style>
