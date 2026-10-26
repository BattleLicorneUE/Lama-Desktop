<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type Info = { keep_background: boolean; autostart: boolean; shortcut: string; shortcut_ok: boolean };
  let info = $state<Info>({ keep_background: false, autostart: false, shortcut: "Ctrl+Alt+S", shortcut_ok: true });
  let error = $state("");

  onMount(async () => {
    try {
      info = await invoke<Info>("app_settings_get");
    } catch (e) {
      error = String(e);
    }
  });

  async function save() {
    try {
      info = await invoke<Info>("app_settings_set", { keepBackground: info.keep_background, autostart: info.autostart });
      error = "";
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section>
  <h3>Démarrage et arrière-plan</h3>
  <label class="check">
    <input type="checkbox" bind:checked={info.keep_background} onchange={save} />
    <span>Garder Lama Desktop actif quand je ferme la fenêtre<br /><span class="muted">Il reste dans la zone de notification (près de l'horloge) : les rappels continuent de sonner. Pour quitter vraiment : clic droit sur l'icône, « Quitter ».</span></span>
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={info.autostart} onchange={save} />
    <span>Lancer Lama Desktop au démarrage de Windows<br /><span class="muted">Il démarre discrètement dans la zone de notification.</span></span>
  </label>
  <p class="muted">Arrêt d'urgence : <kbd>{info.shortcut}</kbd> (ou clic droit sur l'icône près de l'horloge) coupe tout de suite les agents, les commandes en arrière-plan et les demandes en attente, même si la fenêtre n'est pas devant.
    {#if !info.shortcut_ok}<span class="bad"> Ce raccourci est déjà pris par une autre appli : utilise l'icône près de l'horloge.</span>{/if}</p>
  {#if error}<p class="bad">{error}</p>{/if}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0; font-size: 14px; }
  .check { display: flex; gap: 8px; align-items: flex-start; font-size: 14px; }
  .muted { color: var(--muted); font-size: 13px; line-height: 1.45; }
  p.muted { margin: 0; }
  kbd { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
  .bad { color: var(--del); font-size: 13px; margin: 0; }
</style>
