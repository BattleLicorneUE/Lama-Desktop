<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import type { ChatStore } from "$lib/chat.svelte";

  let { chat }: { chat: ChatStore } = $props();

  type Entry = { rank: number; name: string; source: string; id: string; installs: string };
  type Found = { name: string; description: string; dir: string; group: string; size: number; installed: boolean };
  type RepoSkills = { owner: string; repo: string; wanted: string | null; skills: Found[] };
  type Report = { imported: string[]; skipped: string[]; errors: string[] };

  let msg = $state("");
  let busy = $state(false);

  // --- skills.sh ---
  let view = $state<"all" | "trending" | "hot">("all");
  let entries = $state<Entry[]>([]);
  let hubError = $state("");
  let hubLoading = $state(false);
  let filter = $state("");
  let paste = $state("");

  // Skills proposés à l'installation, en attente de confirmation.
  let review = $state<{ title: string; source: string; items: Found[]; chosen: Record<string, boolean> } | null>(null);

  // --- skills de Claude déjà présents sur le PC ---

  // --- création ---
  let skillName = $state("");
  let skillDesc = $state("");

  const shown = $derived(
    entries.filter((e) => !filter.trim() || `${e.name} ${e.source}`.toLowerCase().includes(filter.trim().toLowerCase())),
  );
  const looksLikeSource = $derived(/^\s*(npx\s|https?:\/\/|[\w.-]+\/[\w.-]+)/i.test(filter));
  const kb = (n: number) => `${Math.max(1, Math.round(n / 1000))} Ko`;

  async function loadHub(v = view) {
    view = v;
    hubLoading = true;
    hubError = "";
    try {
      entries = await invoke<Entry[]>("skills_hub_browse", { view: v });
    } catch (e) {
      hubError = String(e);
    } finally {
      hubLoading = false;
    }
  }

  onMount(() => {
    chat.loadSkills();
    loadHub("all");
  });

  /** Télécharge le dépôt et propose les skills trouvés (rien n'est installé avant confirmation). */
  async function prepare(input: string) {
    busy = true;
    msg = "Téléchargement depuis GitHub…";
    try {
      const res = await invoke<RepoSkills>("skills_hub_fetch", { input });
      const wanted = res.wanted?.toLowerCase();
      const match = wanted
        ? res.skills.filter((s) => s.name.toLowerCase() === wanted || s.dir.toLowerCase().replace(/\\/g, "/").endsWith("/" + wanted))
        : [];
      const chosen: Record<string, boolean> = {};
      for (const s of res.skills) {
        chosen[s.dir] = !s.installed && (match.length ? match.includes(s) : res.skills.length <= 3);
      }
      review = {
        title: match.length === 1 ? match[0].name : `${res.owner}/${res.repo}`,
        source: `${res.owner}/${res.repo}`,
        items: res.skills,
        chosen,
      };
      msg = "";
    } catch (e) {
      msg = String(e);
    } finally {
      busy = false;
    }
  }

  async function importDirs(dirs: string[]) {
    if (!dirs.length) return;
    busy = true;
    try {
      const r = await invoke<Report>("skills_import", { dirs });
      const parts = [];
      if (r.imported.length) parts.push(`${r.imported.length} installé${r.imported.length > 1 ? "s" : ""} (${r.imported.join(", ")})`);
      if (r.skipped.length) parts.push(`${r.skipped.length} déjà présent${r.skipped.length > 1 ? "s" : ""}`);
      if (r.errors.length) parts.push(r.errors.join(" ; "));
      msg = parts.join(" — ") || "Rien à installer.";
      await chat.loadSkills();
    } catch (e) {
      msg = String(e);
    } finally {
      busy = false;
    }
  }

  async function confirmReview() {
    if (!review) return;
    const dirs = review.items.filter((s) => review!.chosen[s.dir]).map((s) => s.dir);
    review = null;
    await importDirs(dirs);
  }

  async function toggle(name: string, enabled: boolean) {
    await invoke("skill_set_enabled", { name, enabled });
    await chat.loadSkills();
  }

  async function createSkill() {
    try {
      await invoke("skill_create", { name: skillName, description: skillDesc });
      skillName = skillDesc = "";
      await chat.loadSkills();
      msg = "Skill créé : complète le fichier SKILL.md qui vient de s'ouvrir.";
    } catch (e) {
      msg = String(e);
    }
  }

  const open = (url: string) => openUrl(url).catch(() => {});
  const enabledCount = $derived(chat.skills.filter((s) => s.enabled).length);
  let skillQuery = $state("");
  const shownSkills = $derived.by(() => {
    const q = skillQuery.trim().toLowerCase();
    return q ? chat.skills.filter((s) => (s.name + " " + s.description).toLowerCase().includes(q)) : chat.skills;
  });
</script>

<div class="panel">
  <p class="muted">
    Un skill est un dossier avec un fichier <code>SKILL.md</code> (format de Claude). Le modèle voit un index compact de tous les skills activés (descriptions raccourcies selon la place disponible, les plus pertinents pour ta demande détaillés), cherche les autres avec <code>search_skills</code>, puis charge celui qu'il choisit ; <code>/nom</code> force un skill. Le nombre de skills n'est pas limité. Un skill peut contenir des scripts que l'agent exécute <strong>avec tes autorisations</strong> : n'installe que des sources de confiance.
  </p>
  {#if msg}<p class="msg">{msg}</p>{/if}

  <!-- ===== Installés ===== -->
  <section>
    <div class="head">
      <h3>Installés ({enabledCount} activés / {chat.skills.length})</h3>
      <div class="row">
        <button onclick={() => invoke("skills_open_dir")}>Ouvrir le dossier</button>
        <button onclick={() => chat.loadSkills()}>Actualiser</button>
      </div>
    </div>
    <input class="find" bind:value={skillQuery} placeholder="Rechercher parmi {chat.skills.length} skills…" aria-label="Rechercher un skill" spellcheck="false" />
    <ul class="scroll">
      {#each shownSkills as s (s.dir)}
        <li>
          <label class="sw" title={s.enabled ? "Désactiver" : "Activer"}>
            <input type="checkbox" checked={s.enabled} onchange={(e) => toggle(s.name, e.currentTarget.checked)} />
          </label>
          <div class="grow">
            <strong>/{s.name}</strong><span class="badge">{s.scope === "app" ? "global" : "projet"}</span>{#if s.model_invocable === false}<span class="badge" title="Le modèle ne le choisit pas seul : à lancer avec /{s.name}">manuel</span>{/if}{#if s.argument_hint}<span class="badge" title="Arguments : /{s.name} {s.argument_hint}">{s.argument_hint}</span>{/if}
            <div class="muted clip">{s.description || "Sans description"}</div>
          </div>
        </li>
      {:else}
        <li class="muted">{chat.skills.length ? "Aucun skill ne correspond." : "Aucun skill installé. Installe-en depuis skills.sh ou importe ceux de Claude ci-dessous."}</li>
      {/each}
    </ul>
  </section>

  <!-- ===== skills.sh ===== -->
  <section>
    <div class="head">
      <h3>skills.sh</h3>
      <button onclick={() => open("https://skills.sh")}>Ouvrir le site ↗</button>
    </div>
    <form class="row" onsubmit={(e) => { e.preventDefault(); if (paste.trim()) prepare(paste); }}>
      <input class="grow" bind:value={paste} placeholder="Colle une commande (npx skills add …), une URL skills.sh / GitHub, ou owner/repo" disabled={busy} />
      <button class="primary" disabled={!paste.trim() || busy}>Ajouter</button>
    </form>

    <div class="row">
      <div class="seg">
        {#each [["all", "Populaires"], ["trending", "Tendances"], ["hot", "Hot"]] as [id, label]}
          <button class:on={view === id} onclick={() => loadHub(id as "all" | "trending" | "hot")}>{label}</button>
        {/each}
      </div>
      <input class="grow" bind:value={filter} placeholder="Filtrer la liste…" />
    </div>
    {#if looksLikeSource && filter.trim()}
      <button class="link" onclick={() => prepare(filter)} disabled={busy}>Chercher « {filter.trim()} » sur GitHub →</button>
    {/if}

    {#if hubError}<p class="warn">{hubError}</p>{/if}
    <ul class="scroll tall">
      {#if hubLoading}<li class="muted">Chargement du classement…</li>{/if}
      {#each shown as e (e.id)}
        {@const done = chat.skills.some((s) => s.name.toLowerCase() === e.name.toLowerCase())}
        <li>
          <span class="rank">{e.rank}</span>
          <div class="grow">
            <strong>{e.name}</strong>
            <div class="muted clip">{e.source} · {e.installs} installations</div>
          </div>
          <button onclick={() => prepare(e.id)} disabled={busy || done}>{done ? "Installé" : "Installer"}</button>
        </li>
      {:else}
        {#if !hubLoading}<li class="muted">Aucun résultat dans le classement. Colle un dépôt GitHub ci-dessus pour en installer d'autres.</li>{/if}
      {/each}
    </ul>
    <p class="muted">Le classement est lu depuis la page publique de skills.sh ; les skills sont téléchargés depuis leur dépôt GitHub.</p>
  </section>

  <!-- ===== création ===== -->
  <section>
    <h3>Créer un skill</h3>
    <div class="stack">
      <input bind:value={skillName} placeholder="Nom, ex. revue-de-code" />
      <input bind:value={skillDesc} placeholder="Quand l'utiliser ? (description courte)" />
      <button class="primary" disabled={!skillName.trim() || !skillDesc.trim()} onclick={createSkill}>Créer et ouvrir</button>
    </div>
  </section>
</div>

{#if review}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay top" onmousedown={(e) => e.target === e.currentTarget && (review = null)}>
    <div class="dialog confirm" role="dialog" aria-modal="true" aria-label="Installer des skills">
      <h3>Installer depuis {review.source}</h3>
      <p class="warn">
        Ces skills viennent d'un tiers. Ils peuvent contenir des instructions et des scripts que l'agent pourra exécuter. Vérifie la source avant d'installer.
        <button class="link" onclick={() => open(`https://skills.sh/${review!.source}`)}>Voir la fiche et les audits ↗</button>
      </p>
      <ul class="scroll">
        {#each review.items as s (s.dir)}
          <li>
            <input type="checkbox" disabled={s.installed} bind:checked={review.chosen[s.dir]} aria-label="Installer {s.name}" />
            <div class="grow">
              <strong>{s.name}</strong>{#if s.installed}<span class="badge">déjà installé</span>{/if}<span class="badge">{kb(s.size)}</span>
              <div class="muted clip">{s.description || "Sans description"}</div>
            </div>
          </li>
        {/each}
      </ul>
      <div class="row end">
        <button onclick={() => (review = null)}>Annuler</button>
        <button class="primary" disabled={!Object.values(review.chosen).some(Boolean)} onclick={confirmReview}>
          Installer ({Object.values(review.chosen).filter(Boolean).length})
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .panel { display: flex; flex-direction: column; gap: 18px; }
  section { display: flex; flex-direction: column; gap: 10px; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 10px; }
  h3 { margin: 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .msg { margin: 0; font-size: 13px; color: var(--accent); }
  .warn { margin: 0; font-size: 13px; color: var(--del); line-height: 1.45; }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row.end { justify-content: flex-end; }
  .stack { display: flex; flex-direction: column; gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .clip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  input:not([type="checkbox"]) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 7px 10px; outline: none; }
  input:focus { border-color: var(--accent); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; }
  button:disabled { opacity: 0.45; }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .link { border: 0; background: none; color: var(--accent); padding: 0; text-align: left; text-decoration: underline; }
  .seg { display: inline-flex; background: var(--hover); border-radius: 10px; padding: 3px; }
  .seg button { border: 0; background: none; padding: 4px 11px; border-radius: 8px; color: var(--muted); }
  .seg button.on { background: var(--surface); color: var(--fg); }
  ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 10px; }
  ul.scroll, .scroll { max-height: 190px; overflow-y: auto; }
  .tall { max-height: 260px; }
  li { display: flex; align-items: center; gap: 10px; padding: 7px 12px; border-bottom: 1px solid var(--border); font-size: 14px; }
  li:last-child { border-bottom: 0; }
  .rank { width: 28px; color: var(--muted); font-size: 12px; text-align: right; font-variant-numeric: tabular-nums; }
  .badge { font-size: 11px; background: var(--hover); color: var(--muted); border-radius: 5px; padding: 0 6px; margin-left: 6px; }
  .sw input { accent-color: var(--accent); }
  .confirm { max-width: 480px; display: flex; flex-direction: column; gap: 12px; }
  .confirm .scroll { max-height: 280px; }
</style>
