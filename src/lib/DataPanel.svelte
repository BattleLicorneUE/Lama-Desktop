<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import type { ChatStore } from "$lib/chat.svelte";
  import { askConfirm } from "$lib/dialogs.svelte";

  let { chat }: { chat: ChatStore } = $props();

  let msg = $state("");
  let bad = $state(false);
  let busy = $state(false);
  let confirmText = $state("");

  const stamp = () => new Date().toISOString().slice(0, 10);
  const plural = (n: number, w: string) => `${n} ${w}${n > 1 ? "s" : ""}`;

  async function run(job: () => Promise<string | null>) {
    busy = true;
    msg = "";
    bad = false;
    try {
      msg = (await job()) ?? "";
    } catch (e) {
      msg = String(e);
      bad = true;
    } finally {
      busy = false;
    }
  }

  const backup = () =>
    run(async () => {
      const path = await saveDialog({ title: "Enregistrer la sauvegarde", defaultPath: `Lama-sauvegarde-${stamp()}.zip`, filters: [{ name: "Archive Lama", extensions: ["zip"] }] });
      if (!path) return null;
      const n = await invoke<number>("data_backup", { path });
      chat.track({ kind: "backup" });
      return `Sauvegarde créée (${plural(n, "fichier")}) : ${path}`;
    });

  const restore = () =>
    run(async () => {
      if (chat.anyRunning) throw new Error("Une réponse est en cours : attends sa fin ou arrête-la avant de restaurer.");
      const path = await openDialog({ title: "Choisir une sauvegarde", multiple: false, filters: [{ name: "Archive Lama", extensions: ["zip"] }] });
      if (typeof path !== "string") return null;
      const ok = await askConfirm("Remplacer les conversations, projets, mémoire et réglages de ce profil par ceux de la sauvegarde ? Ta base actuelle est gardée à côté (history-avant-restauration.db).", "Restaurer");
      if (!ok) return null;
      // Les serveurs MCP lancent des programmes : on ne les restaure que si tu les as lus et acceptés.
      const servers = await invoke<string[]>("data_restore_preview", { path });
      const withMcp = servers.length > 0 && (await askConfirm(`Cette sauvegarde contient des serveurs MCP, qui lancent des programmes sur ton ordinateur :\n\n${servers.join("\n")}\n\nLes restaurer aussi ? Ne le fais que si tu as créé cette sauvegarde toi-même.`, "Restaurer les serveurs"));
      await invoke("data_restore", { path, withMcp });
      location.reload();
      return "Sauvegarde restaurée, rechargement…";
    });

  const exportChats = () =>
    run(async () => {
      const path = await saveDialog({ title: "Exporter les conversations", defaultPath: `Lama-conversations-${stamp()}.zip`, filters: [{ name: "Archive", extensions: ["zip"] }] });
      if (!path) return null;
      const n = await invoke<number>("data_export_chats", { path });
      return `${plural(n, "conversation")} exportée${n > 1 ? "s" : ""} en Markdown : ${path}`;
    });

  const erase = () =>
    run(async () => {
      if (chat.anyRunning) throw new Error("Une réponse est en cours : arrête-la avant d'effacer.");
      const ok = await askConfirm("Effacer TOUTES les données de Lama Desktop (profils, conversations, projets, mémoire, réglages, clés) ? Cette action est définitive.", "Tout effacer");
      if (!ok) return null;
      // Les préférences de l'interface partent aussi ; on les remet si l'effacement échoue.
      const kept: [string, string][] = [];
      try {
        for (let i = 0; i < localStorage.length; i++) {
          const k = localStorage.key(i)!;
          kept.push([k, localStorage.getItem(k) ?? ""]);
        }
        localStorage.clear();
      } catch {}
      try {
        await invoke("data_erase", { confirm: confirmText });
      } catch (e) {
        try {
          for (const [k, v] of kept) localStorage.setItem(k, v);
        } catch {}
        throw e;
      }
      return "Données effacées, redémarrage…";
    });
</script>

<section>
  <h3>Sauvegarde</h3>
  <p class="muted">
    Une archive .zip du profil ouvert : conversations, projets et leurs documents, mémoire, artéfacts, tâches planifiées, réglages, skills, agents et configuration des connecteurs.
    Les clés et jetons restent dans le coffre Windows : ils ne sont pas dans la sauvegarde, il faudra les ressaisir sur un autre PC.
  </p>
  <div class="row">
    <button onclick={backup} disabled={busy}>Créer une sauvegarde…</button>
    <button onclick={restore} disabled={busy}>Restaurer une sauvegarde…</button>
  </div>
</section>

<section>
  <h3>Exporter mes conversations</h3>
  <p class="muted">Toutes les conversations (chat et code) en fichiers Markdown lisibles partout, rangées dans un dossier par projet. L'export d'une seule conversation est dans son menu, dans la liste.</p>
  <div class="row"><button onclick={exportChats} disabled={busy}>Exporter tout en Markdown…</button></div>
</section>

{#if msg}<p class="msg" class:bad role="status">{msg}</p>{/if}

<section class="danger">
  <h3>Tout effacer</h3>
  <p class="muted">
    Supprime définitivement toutes les données de Lama Desktop sur ce PC, <b>tous profils confondus</b> : conversations, projets, mémoire, tâches, réglages, skills, agents, connecteurs,
    clés enregistrées dans le coffre Windows et lancement avec Windows. L'application redémarre comme au premier jour.
    Les documents que l'agent a créés dans <code>Documents\Lama Desktop</code> et tes propres dossiers de travail ne sont pas touchés.
  </p>
  <label class="field">
    <span>Pour confirmer, tape <code>EFFACER</code></span>
    <input bind:value={confirmText} autocomplete="off" spellcheck="false" placeholder="EFFACER" />
  </label>
  <div class="row"><button class="del" onclick={erase} disabled={busy || confirmText.trim() !== "EFFACER"}>Effacer toutes mes données</button></div>
</section>

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; }
  .msg { margin: 0; font-size: 13.5px; color: var(--add); word-break: break-word; }
  .msg.bad { color: var(--del); }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); font: inherit; }
  button:disabled { opacity: 0.45; }
  .danger { border: 1px solid color-mix(in srgb, var(--del) 45%, var(--border)); border-radius: 12px; padding: 16px; }
  .danger h3 { color: var(--del); }
  .field { display: flex; flex-direction: column; gap: 5px; font-size: 13px; color: var(--muted); max-width: 280px; }
  .field input { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 7px 10px; outline: none; color: var(--fg); }
  .field input:focus { border-color: var(--del); }
  button.del { border-color: var(--del); background: var(--del); color: #fff; }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12.5px; }
</style>
