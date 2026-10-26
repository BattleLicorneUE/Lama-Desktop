<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import Icon from "$lib/Icon.svelte";

  type Pc = { enabled: boolean; dirs: string[] };
  type Entry = { t: number; tool: string; summary: string; outcome: string };
  let journal = $state<Entry[]>([]);
  const loadJournal = () => invoke<Entry[]>("journal_list", { limit: 60 }).then((j) => (journal = j)).catch(() => {});
  const OUTCOME: Record<string, string> = { done: "fait", error: "échec", denied: "refusé" };
  const when = (t: number) => new Date(t * 1000).toLocaleString("fr-FR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" });
  async function clearJournal() {
    await invoke("journal_clear");
    journal = [];
  }
  type Reminder = { id: string; title: string; body: string; at: number };
  let reminders = $state<Reminder[]>([]);
  const loadReminders = () => invoke<Reminder[]>("reminders_list").then((r) => (reminders = r)).catch(() => {});
  const at = (r: Reminder) => new Date(r.at * 1000).toLocaleString("fr-FR", { weekday: "short", hour: "2-digit", minute: "2-digit" });
  let enabled = $state(true);
  let dirs = $state<string[]>([]);
  let error = $state("");
  let ready = $state(false);

  onMount(() => {
    (async () => {
      try {
        const s = await invoke<Pc>("pc_get");
        enabled = s.enabled;
        dirs = s.dirs;
      } catch (e) {
        error = String(e);
      }
      await loadReminders();
      await loadJournal();
      ready = true;
    })();
    const timer = setInterval(() => { loadReminders(); loadJournal(); }, 10_000);
    return () => clearInterval(timer);
  });

  async function save() {
    try {
      const s = await invoke<Pc>("pc_set", { enabled, dirs: $state.snapshot(dirs) });
      enabled = s.enabled;
      dirs = s.dirs;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function add() {
    const sel = await openDialog({ directory: true, multiple: false, title: "Dossier à autoriser" });
    if (typeof sel === "string" && !dirs.includes(sel)) {
      dirs = [...dirs, sel];
      await save();
    }
  }

  async function remove(d: string) {
    dirs = dirs.filter((x) => x !== d);
    await save();
  }

  const ABILITIES = [
    ["Heure et infos du PC", "date, disques, batterie, processus, applications installées"],
    ["Ouvrir", "un site, un fichier, un dossier ou une application"],
    ["Presse-papiers", "lire ou copier du texte"],
    ["Réglages", "volume, sourdine, thème sombre, luminosité, verrouillage"],
    ["Voir l'écran", "faire une capture d'écran (le modèle doit savoir lire les images)"],
    ["Souris et clavier", "cliquer, saisir du texte, raccourcis"],
  ];
</script>

<section>
  <label class="check">
    <input type="checkbox" bind:checked={enabled} onchange={save} disabled={!ready} />
    <span><strong>Laisser Lama agir sur ce PC</strong><br />
      <span class="muted">Chaque action te demande ton accord (Autoriser ou Refuser), sauf si tu choisis « Ignorer les permissions » dans le menu Mode. Le bouton Stop arrête tout.</span></span>
  </label>
  <ul class="abilities" class:off={!enabled}>
    {#each ABILITIES as [name, desc]}
      <li><strong>{name}</strong><span class="muted"> — {desc}</span></li>
    {/each}
  </ul>
</section>

<section>
  <h3>Dossiers autorisés</h3>
  <p class="muted">En plus du dossier de travail du mode Code, le modèle peut lire, chercher et modifier des fichiers dans ces dossiers (avec ton accord à chaque fois pour modifier), y compris en mode Chat. Rien en dehors.</p>
  <ul>
    {#each dirs as d (d)}
      <li><span class="grow path" title={d}>{d}</span>
        <button class="del" aria-label="Retirer ce dossier" onclick={() => remove(d)}><Icon name="close" size={14} /></button></li>
    {:else}
      <li class="muted">Aucun dossier pour l'instant.</li>
    {/each}
  </ul>
  <div class="row"><button onclick={add}>Ajouter un dossier…</button></div>
  {#if error}<p class="err">{error}</p>{/if}
</section>

<section>
  <h3>Rappels programmés</h3>
  <p class="muted">Demande par exemple « rappelle-moi dans 20 minutes de sortir le gratin ». Le rappel ne sonne que si Lama Desktop est ouvert, sinon il sonne à la prochaine ouverture.</p>
  <ul>
    {#each reminders as r (r.id)}
      <li><span class="grow"><strong>{r.title}</strong> <span class="muted">— {r.body} · {at(r)}</span></span>
        <button class="del" aria-label="Annuler ce rappel" onclick={async () => { await invoke("reminders_cancel", { id: r.id }); await loadReminders(); }}><Icon name="close" size={14} /></button></li>
    {:else}
      <li class="muted">Aucun rappel en attente.</li>
    {/each}
  </ul>
</section>

<section>
  <h3>Journal des actions</h3>
  <p class="muted">Ce que l'agent a fait sur ton PC (commandes, fichiers, souris, presse-papiers…). Le texte tapé ou copié n'est jamais enregistré, seulement sa taille.</p>
  <ul class="journal">
    {#each journal as e}
      <li><span class="when">{when(e.t)}</span><strong>{e.tool}</strong><span class="grow sum" title={e.summary}>{e.summary}</span><em class={e.outcome}>{OUTCOME[e.outcome] ?? e.outcome}</em></li>
    {:else}
      <li class="muted">Aucune action enregistrée.</li>
    {/each}
  </ul>
  {#if journal.length}<div class="row"><button onclick={clearJournal}>Effacer le journal</button></div>{/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .check { display: flex; gap: 10px; align-items: flex-start; font-size: 14px; }
  .abilities { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; font-size: 13.5px; }
  .abilities.off { opacity: 0.45; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { display: flex; align-items: center; gap: 8px; }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 12.5px; background: var(--code); border-radius: 6px; padding: 4px 8px; }
  .grow { flex: 1; min-width: 0; }
  .row { display: flex; gap: 8px; }
  .err { color: var(--del); font-size: 13px; margin: 0; }
  input:not([type="checkbox"]) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 7px 10px; outline: none; font-size: 13.5px; }
  input:focus { border-color: var(--accent); }
  .presets { flex-wrap: wrap; }
  .journal { max-height: 220px; overflow-y: auto; font-size: 12.5px; }
  .journal li { gap: 8px; }
  .journal .when { color: var(--muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .journal strong { font-weight: 600; white-space: nowrap; }
  .journal .sum { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); }
  .journal em { font-style: normal; font-size: 12px; }
  .journal em.done { color: var(--add); }
  .journal em.error, .journal em.denied { color: var(--del); }
  .del { border: 0; background: none; color: var(--muted); padding: 2px; }
  button:not(.del) { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; align-self: flex-start; }
</style>
