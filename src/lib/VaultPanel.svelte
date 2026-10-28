<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { askConfirm } from "$lib/dialogs.svelte";
  import Icon from "$lib/Icon.svelte";
  import { DEFAULT_GEN, INFO_PRESETS, countdown, strength, type GenOpts, type Info, type Login, type LoginView, type VaultStatus } from "$lib/vault";

  let status = $state<VaultStatus | null>(null);
  let br = $state({ available: true, name: "", running: false });
  let logins = $state<LoginView[]>([]);
  let infos = $state<Info[]>([]);
  let tab = $state<"logins" | "infos" | "security">("logins");
  let msg = $state("");
  let bad = $state(false);
  let busy = $state(false);
  let search = $state("");

  // Création / ouverture
  let master = $state("");
  let master2 = $state("");
  // Sécurité
  let oldMaster = $state("");
  let newMaster = $state("");
  let destroyMaster = $state("");

  // Compte à rebours local du verrouillage automatique (le serveur fait foi à chaque action).
  let locksAt = $state(0);
  let now = $state(Date.now());
  const left = $derived(status?.unlocked ? Math.max(0, (locksAt - now) / 1000) : 0);

  function say(text: string, error = false) {
    msg = text;
    bad = error;
  }

  async function refresh() {
    try {
      br = await invoke<typeof br>("browser_status").catch(() => br);
      status = await invoke<VaultStatus>("vault_status");
      locksAt = Date.now() + status.locks_in * 1000;
      if (status.unlocked) {
        const l = await invoke<{ logins: LoginView[]; infos: Info[] }>("vault_list");
        logins = l.logins;
        infos = l.infos;
      } else {
        logins = [];
        infos = [];
      }
    } catch (e) {
      say(String(e), true);
    }
  }

  onMount(() => {
    refresh();
    const tick = setInterval(() => (now = Date.now()), 1000);
    const un = listen("vault:locked", () => {
      say("Le coffre s'est verrouillé (inactivité).");
      editor = null;
      refresh();
    });
    return () => {
      clearInterval(tick);
      un.then((f) => f());
      // Quitter la page ne verrouille pas ; le minuteur d'inactivité s'en charge.
    };
  });

  async function run(job: () => Promise<string | void>) {
    busy = true;
    msg = "";
    try {
      const out = await job();
      if (out) say(out);
    } catch (e) {
      say(String(e), true);
    } finally {
      busy = false;
      await refresh();
    }
  }

  const create = () =>
    run(async () => {
      if (master !== master2) throw "Les deux mots de passe maîtres ne correspondent pas.";
      await invoke("vault_create", { master });
      master = master2 = "";
      return "Coffre créé. Note bien ton mot de passe maître : il est impossible de le récupérer.";
    });

  const unlock = () =>
    run(async () => {
      await invoke("vault_unlock", { master });
      master = "";
    });

  const lock = () =>
    run(async () => {
      await invoke("vault_lock");
      editor = null;
      return "Coffre verrouillé.";
    });

  // ---------------------------------------------------------------- identifiants
  const shown = $derived(
    logins.filter((l) => !search.trim() || `${l.title} ${l.url} ${l.username}`.toLowerCase().includes(search.trim().toLowerCase())),
  );

  type Editor = Login & { reveal: boolean };
  let editor = $state<Editor | null>(null);
  let gen = $state<GenOpts>(loadGen());
  let genOpen = $state(false);
  let genBits = $state(0);

  function loadGen(): GenOpts {
    try {
      return { ...DEFAULT_GEN, ...JSON.parse(localStorage.getItem("vault-gen") ?? "{}") };
    } catch {
      return { ...DEFAULT_GEN };
    }
  }

  const blank = (): Editor => ({ id: "", title: "", url: "", username: "", password: "", notes: "", created: 0, updated: 0, reveal: false });

  async function edit(id: string) {
    await run(async () => {
      editor = { ...(await invoke<Login>("vault_login_get", { id })), reveal: false };
      genOpen = false;
    });
  }

  async function regenerate() {
    if (!editor) return;
    try {
      localStorage.setItem("vault-gen", JSON.stringify(gen));
    } catch {}
    const r = await invoke<{ password: string; bits: number }>("vault_generate", { opts: gen }).catch((e) => {
      say(String(e), true);
      return null;
    });
    if (r) {
      editor.password = r.password;
      editor.reveal = true;
      genBits = r.bits;
    }
  }

  const saveLogin = () =>
    run(async () => {
      if (!editor) return;
      const { reveal: _r, ...login } = editor;
      await invoke("vault_login_save", { login });
      editor = null;
      return "Enregistré dans le coffre.";
    });

  async function removeLogin(l: LoginView) {
    if (!(await askConfirm(`Supprimer « ${l.title} » du coffre ?`))) return;
    await run(async () => {
      await invoke("vault_login_delete", { id: l.id });
      return "Entrée supprimée.";
    });
  }

  async function copy(id: string, field: "password" | "username" | "info") {
    await run(async () => {
      const s = await invoke<number>("vault_copy", { id, field });
      return `Copié. Effacé du presse-papiers dans ${s} s (et absent de l'historique Windows).`;
    });
  }

  async function fill(items: { kind: string; id: string }[], label: string) {
    await run(async () => {
      say(`Lama se réduit : clique dans le champ à remplir (« ${label} »), la saisie commence dans 4 s…`);
      const r = await invoke<string>("vault_fill_ui", { resolved: { items, wait: 4, submit: false, tab: true } });
      return r;
    });
  }

  /** Ouvre le site de l'entrée dans le navigateur de Lama et s'y connecte (identifiant, mot de passe, Entrée). */
  async function connect(l: LoginView) {
    await run(async () => {
      say(`Ouverture de ${l.title} dans le navigateur de Lama…`);
      return await invoke<string>("vault_browser_login", { id: l.id, submit: true });
    });
  }

  const openBrowser = () => run(async () => (await invoke<string>("browser_launch_ui", { url: null })) + " Connecte-toi à la main (code de vérification compris) : la session reste ouverte pour l'assistant.");
  const closeBrowser = () => run(async () => await invoke<string>("browser_close_ui"));
  async function wipeBrowser() {
    if (!(await askConfirm("Effacer les sessions, cookies et le cache du navigateur de Lama ? Tu seras déconnecté de tous les sites.", "Effacer"))) return;
    await run(async () => {
      await invoke("browser_wipe");
      return "Navigateur de Lama réinitialisé.";
    });
  }

  // ---------------------------------------------------------------- infos rapides
  let infoEditor = $state<Info | null>(null);
  const missingPresets = $derived(INFO_PRESETS.filter((p) => !infos.some((i) => i.label.toLowerCase() === p.toLowerCase())));
  let picked = $state<Record<string, boolean>>({});
  const pickedItems = $derived(infos.filter((i) => picked[i.id]).map((i) => ({ kind: "info", id: i.id })));

  const saveInfo = () =>
    run(async () => {
      if (!infoEditor) return;
      await invoke("vault_info_save", { info: infoEditor });
      infoEditor = null;
      return "Enregistré.";
    });

  async function removeInfo(i: Info) {
    if (!(await askConfirm(`Supprimer « ${i.label} » ?`))) return;
    await run(async () => {
      await invoke("vault_info_delete", { id: i.id });
    });
  }

  // ---------------------------------------------------------------- sécurité
  const setAgent = (on: boolean) => run(async () => void (await invoke("vault_set_agent_access", { on })));

  const changeMaster = () =>
    run(async () => {
      await invoke("vault_change_master", { old: oldMaster, new: newMaster });
      oldMaster = newMaster = "";
      return "Mot de passe maître changé : le coffre a été rechiffré.";
    });

  async function destroy() {
    if (!(await askConfirm("Supprimer définitivement le coffre et tout son contenu ? Impossible à annuler.", "Supprimer le coffre"))) return;
    await run(async () => {
      await invoke("vault_destroy", { master: destroyMaster });
      destroyMaster = "";
      return "Coffre supprimé.";
    });
  }

  const mStrength = $derived(strength(master));
  const eStrength = $derived(strength(editor?.password ?? ""));
  const nStrength = $derived(strength(newMaster));
</script>

<section>
  
  <p class="muted">Tout est chiffré avec ton mot de passe maître. Rien ne quitte ton ordinateur, et le modèle ne voit jamais un secret : le texte est tapé directement au clavier.</p>
</section>

{#if msg}<p class="msg" class:bad role="status">{msg}</p>{/if}

{#if !status}
  <p class="muted">Chargement…</p>
{:else if !status.exists}
  <section class="card">
    <h4>Créer ton coffre</h4>
    <p class="muted">Choisis un mot de passe maître solide, une phrase de plusieurs mots par exemple. Il n'y a aucun moyen de le retrouver : si tu l'oublies, le contenu est perdu.</p>
    <form onsubmit={(e) => { e.preventDefault(); create(); }}>
      <label><span>Mot de passe maître</span><input type="password" bind:value={master} autocomplete="new-password" spellcheck="false" /></label>
      {#if master}<div class="meter" data-level={mStrength.level} title="{mStrength.bits} bits"><span style:width="{(mStrength.level + 1) * 20}%"></span><small>{mStrength.label}</small></div>{/if}
      <label><span>Confirme-le</span><input type="password" bind:value={master2} autocomplete="new-password" spellcheck="false" /></label>
      <div class="row"><button class="primary" disabled={busy || master.length < 10 || master !== master2}>Créer le coffre</button></div>
      <p class="muted">Au moins 10 caractères.</p>
    </form>
  </section>
{:else if !status.unlocked}
  <section class="card">
    <h4>Coffre verrouillé</h4>
    <form onsubmit={(e) => { e.preventDefault(); if (master) unlock(); }}>
      <label><span>Mot de passe maître</span><input type="password" bind:value={master} autocomplete="current-password" spellcheck="false" /></label>
      <div class="row"><button class="primary" disabled={busy || !master}>{busy ? "Vérification…" : "Ouvrir"}</button></div>
      {#if status.failures > 0}<p class="muted">{status.failures} échec{status.failures > 1 ? "s" : ""} : l'attente augmente à chaque erreur.</p>{/if}
    </form>
  </section>
{:else}
  <div class="bar">
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "logins"} class:on={tab === "logins"} onclick={() => (tab = "logins")}>Mots de passe <span class="n">{logins.length}</span></button>
      <button role="tab" aria-selected={tab === "infos"} class:on={tab === "infos"} onclick={() => (tab = "infos")}>Infos rapides <span class="n">{infos.length}</span></button>
      <button role="tab" aria-selected={tab === "security"} class:on={tab === "security"} onclick={() => (tab = "security")}>Sécurité</button>
    </div>
    <span class="auto" title="Le coffre se verrouille tout seul après 5 minutes sans l'utiliser">Verrouillage dans {countdown(left)}</span>
    <button class="lockbtn" onclick={lock} disabled={busy}><Icon name="lock" size={15} /> Verrouiller</button>
  </div>

  {#if tab === "logins"}
    {#if editor}
      <form class="card" onsubmit={(e) => { e.preventDefault(); saveLogin(); }}>
        <h4>{editor.id ? "Modifier" : "Nouvelle entrée"}</h4>
        <label><span>Nom</span><input bind:value={editor.title} placeholder="Ex. GitHub" autocomplete="off" /></label>
        <label><span>Adresse du site</span><input bind:value={editor.url} placeholder="https://github.com" autocomplete="off" spellcheck="false" /></label>
        <label><span>Identifiant ou e-mail</span><input bind:value={editor.username} autocomplete="off" spellcheck="false" /></label>
        <label>
          <span>Mot de passe</span>
          <span class="pw">
            <input type={editor.reveal ? "text" : "password"} bind:value={editor.password} autocomplete="off" spellcheck="false" />
            <button type="button" class="ghost" onclick={() => editor && (editor.reveal = !editor.reveal)}>{editor.reveal ? "Masquer" : "Afficher"}</button>
            <button type="button" class="ghost" onclick={() => { genOpen = !genOpen; if (genOpen) regenerate(); }}>Générer</button>
          </span>
        </label>
        {#if editor.password}<div class="meter" data-level={eStrength.level} title="{eStrength.bits} bits"><span style:width="{(eStrength.level + 1) * 20}%"></span><small>{eStrength.label}</small></div>{/if}
        {#if genOpen}
          <div class="gen">
            <label class="range"><span>Longueur : {gen.length}</span><input type="range" min="8" max="64" bind:value={gen.length} oninput={regenerate} /></label>
            <div class="checks">
              <label><input type="checkbox" bind:checked={gen.upper} onchange={regenerate} /> A–Z</label>
              <label><input type="checkbox" bind:checked={gen.lower} onchange={regenerate} /> a–z</label>
              <label><input type="checkbox" bind:checked={gen.digits} onchange={regenerate} /> 0–9</label>
              <label><input type="checkbox" bind:checked={gen.symbols} onchange={regenerate} /> !@#…</label>
              <label><input type="checkbox" bind:checked={gen.avoid_ambiguous} onchange={regenerate} /> Sans caractères ambigus (I l 1 O 0)</label>
            </div>
            <p class="muted">Généré par le générateur aléatoire sécurisé de Windows · environ {Math.round(genBits)} bits d'entropie. <button type="button" class="link" onclick={regenerate}>Un autre</button></p>
          </div>
        {/if}
        <label><span>Notes</span><textarea rows="2" bind:value={editor.notes}></textarea></label>
        <div class="row end">
          <button type="button" onclick={() => (editor = null)}>Annuler</button>
          <button class="primary" disabled={busy || !editor.title.trim()}>Enregistrer</button>
        </div>
      </form>
    {:else}
      <div class="row">
        <input class="grow" bind:value={search} placeholder="Rechercher…" aria-label="Rechercher dans le coffre" />
        <button class="primary" onclick={() => { editor = blank(); genOpen = false; }}><Icon name="plus" size={15} /> Ajouter</button>
      </div>
      <ul class="list">
        {#each shown as l (l.id)}
          <li>
            <div class="main">
              <b>{l.title}</b>
              <span class="meta">{[l.url.replace(/^https?:\/\//, "").replace(/\/$/, ""), l.username].filter(Boolean).join(" · ") || "—"}</span>
            </div>
            <div class="acts">
              <button class="icon" title={l.url ? "Se connecter : ouvre le site dans le navigateur de Lama et le remplit" : "Ajoute l'adresse du site pour te connecter automatiquement"} aria-label="Se connecter à {l.title}" onclick={() => connect(l)} disabled={busy || !l.url || !br.available}><Icon name="globe" size={16} /></button>
              <button class="icon" title="Remplir : Lama se réduit et tape l'identifiant, Tab, le mot de passe" aria-label="Remplir {l.title}" onclick={() => fill([{ kind: "login", id: l.id }], l.title)} disabled={busy}><Icon name="play" size={16} /></button>
              <button class="icon txt" title="Copier le mot de passe" onclick={() => copy(l.id, "password")} disabled={busy}>Mot de passe</button>
              <button class="icon txt" title="Copier l'identifiant" onclick={() => copy(l.id, "username")} disabled={busy || !l.username}>Identifiant</button>
              <button class="icon" title="Modifier" aria-label="Modifier {l.title}" onclick={() => edit(l.id)} disabled={busy}><Icon name="pencil" size={16} /></button>
              <button class="icon" title="Supprimer" aria-label="Supprimer {l.title}" onclick={() => removeLogin(l)} disabled={busy}><Icon name="trash" size={16} /></button>
            </div>
          </li>
        {:else}
          <li class="empty">{logins.length ? "Aucun résultat." : "Le coffre est vide. Ajoute une entrée, ou génère un mot de passe solide pour un nouveau compte."}</li>
        {/each}
      </ul>
    {/if}
  {:else if tab === "infos"}
    <p class="muted">Des infos que tu retapes souvent. « Remplir » réduit Lama et les tape dans le champ choisi. Tu peux en cocher plusieurs pour remplir un formulaire d'un coup (elles sont séparées par une tabulation, dans l'ordre de la liste).</p>
    {#if infoEditor}
      <form class="card" onsubmit={(e) => { e.preventDefault(); saveInfo(); }}>
        <label><span>Nom de la rubrique</span><input bind:value={infoEditor.label} placeholder="Ex. Téléphone" autocomplete="off" /></label>
        <label><span>Valeur</span><textarea rows="2" bind:value={infoEditor.value}></textarea></label>
        <div class="row end">
          <button type="button" onclick={() => (infoEditor = null)}>Annuler</button>
          <button class="primary" disabled={busy || !infoEditor.label.trim()}>Enregistrer</button>
        </div>
      </form>
    {:else}
      {#if missingPresets.length}
        <div class="chips">
          {#each missingPresets as p}<button class="chip" onclick={() => (infoEditor = { id: "", label: p, value: "" })}>+ {p}</button>{/each}
          <button class="chip" onclick={() => (infoEditor = { id: "", label: "", value: "" })}>+ Autre…</button>
        </div>
      {/if}
      <ul class="list">
        {#each infos as i (i.id)}
          <li>
            <label class="pick"><input type="checkbox" bind:checked={picked[i.id]} aria-label="Remplir {i.label}" /></label>
            <div class="main">
              <b>{i.label}</b>
              <span class="meta val">{i.value || "—"}</span>
            </div>
            <div class="acts">
              <button class="icon" title="Remplir cette rubrique" aria-label="Remplir {i.label}" onclick={() => fill([{ kind: "info", id: i.id }], i.label)} disabled={busy || !i.value}><Icon name="play" size={16} /></button>
              <button class="icon txt" onclick={() => copy(i.id, "info")} disabled={busy || !i.value}>Copier</button>
              <button class="icon" title="Modifier" aria-label="Modifier {i.label}" onclick={() => (infoEditor = { ...i })} disabled={busy}><Icon name="pencil" size={16} /></button>
              <button class="icon" title="Supprimer" aria-label="Supprimer {i.label}" onclick={() => removeInfo(i)} disabled={busy}><Icon name="trash" size={16} /></button>
            </div>
          </li>
        {:else}
          <li class="empty">Aucune information pour l'instant : ajoute ton prénom, ton e-mail, ton adresse… avec les boutons ci-dessus.</li>
        {/each}
      </ul>
      {#if pickedItems.length}
        <div class="row"><button class="primary" disabled={busy} onclick={() => fill(pickedItems, `${pickedItems.length} champs`)}><Icon name="play" size={15} /> Remplir {pickedItems.length} champ{pickedItems.length > 1 ? "s" : ""}</button></div>
      {/if}
    {/if}
  {:else}
    <section class="card">
      <h4>Assistant IA</h4>
      <label class="check">
        <input type="checkbox" checked={status.agent_access} onchange={(e) => setAgent(e.currentTarget.checked)} />
        Autoriser l'assistant à utiliser le coffre
      </label>
      <p class="muted">Désactivé par défaut. Si tu l'actives, l'assistant peut voir les noms de tes entrées (jamais un mot de passe) et <b>demander</b> à remplir un formulaire.
        Avant chaque saisie, tu vois ce qui sera tapé et tu dois dire oui, même en mode « Ignorer les permissions ». Le coffre doit être ouvert.
        L'assistant peut aussi se connecter à un site dans le navigateur de Lama (voir plus bas), à condition que la page soit bien celle de l'entrée.</p>
    </section>

    <section class="card">
      <h4>Navigateur de Lama</h4>
      <p class="muted">C'est une fenêtre {br.name || "Edge, Chrome ou Brave"} à part, avec ses propres sessions : elle ne voit ni tes onglets ni tes mots de passe habituels.
        Connecte-toi une fois à la main (double authentification et captcha compris) : la session reste ouverte et l'assistant n'a souvent plus besoin du mot de passe.
        Pour une connexion automatique, il remplit l'identifiant et le mot de passe sans que le modèle les voie, <b>seulement si l'adresse de la page est bien celle de l'entrée</b> (https, même site).
        Sur un faux site, rien n'est tapé.</p>
      {#if !br.available}
        <p class="msg bad">Aucun navigateur compatible (Edge, Chrome ou Brave) n'est installé sur ce PC.</p>
      {:else}
        <div class="row">
          <button onclick={openBrowser} disabled={busy}>{br.running ? "Afficher le navigateur de Lama" : "Ouvrir le navigateur de Lama"}</button>
          <button onclick={closeBrowser} disabled={busy || !br.running}>Fermer</button>
          <button onclick={wipeBrowser} disabled={busy}>Effacer ses sessions…</button>
        </div>
      {/if}
      <p class="muted">Ses données restent dans ton profil, hors des sauvegardes, et se ferment avec Lama.</p>
    </section>

    <section class="card">
      <h4>Changer le mot de passe maître</h4>
      <form onsubmit={(e) => { e.preventDefault(); changeMaster(); }}>
        <label><span>Ancien</span><input type="password" bind:value={oldMaster} autocomplete="current-password" /></label>
        <label><span>Nouveau</span><input type="password" bind:value={newMaster} autocomplete="new-password" /></label>
        {#if newMaster}<div class="meter" data-level={nStrength.level}><span style:width="{(nStrength.level + 1) * 20}%"></span><small>{nStrength.label}</small></div>{/if}
        <div class="row"><button class="primary" disabled={busy || !oldMaster || newMaster.length < 10}>Changer</button></div>
      </form>
    </section>

    <section class="card danger">
      <h4>Supprimer le coffre</h4>
      <p class="muted">Efface le fichier chiffré de ce profil. Il n'y a pas de corbeille ni de récupération.</p>
      <form onsubmit={(e) => { e.preventDefault(); destroy(); }}>
        <label><span>Mot de passe maître</span><input type="password" bind:value={destroyMaster} autocomplete="current-password" /></label>
        <div class="row"><button class="del" disabled={busy || !destroyMaster}>Supprimer définitivement</button></div>
      </form>
    </section>

    <section class="muted">
      <b>Comment c'est protégé</b>
      <ul class="how">
        <li>Chiffrement authentifié XChaCha20-Poly1305 ; clé dérivée de ton mot de passe par Argon2id (64 Mo, 3 passes) : deviner le mot de passe coûte cher même à un attaquant qui a copié le fichier.</li>
        <li>Le mot de passe maître et la clé ne sont jamais écrits sur le disque. Le coffre se verrouille après 5 minutes d'inactivité, au changement de profil et à la fermeture de Lama.</li>
        <li>La copie dans le presse-papiers est exclue de l'historique et de la synchronisation Windows, puis effacée après 30 secondes.</li>
        <li>Le fichier chiffré est inclus dans tes sauvegardes : sans le mot de passe maître, il reste illisible.</li>
        <li>Limite : un logiciel malveillant déjà installé sur ton PC, qui capte le clavier ou la mémoire pendant que le coffre est ouvert, peut tout voir. Garde Windows à jour et verrouille le coffre après usage.</li>
      </ul>
    </section>
  {/if}
{/if}

<style>
  section { display: flex; flex-direction: column; gap: 10px; }
  h4 { margin: 0; font-size: 15px; font-weight: 600; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  .msg { margin: 0; font-size: 13.5px; color: var(--add); word-break: break-word; }
  .msg.bad { color: var(--del); }
  .card { border: 1px solid var(--border); background: var(--surface); border-radius: 14px; padding: 16px 18px; display: flex; flex-direction: column; gap: 12px; }
  .card.danger { border-color: color-mix(in srgb, var(--del) 45%, var(--border)); }
  form { display: flex; flex-direction: column; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 5px; font-size: 13px; color: var(--muted); min-width: 0; }
  label.check { flex-direction: row; align-items: center; gap: 8px; color: var(--fg); font-size: 14px; }
  input:not([type="checkbox"]):not([type="range"]), textarea { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 7px 10px; outline: none; color: var(--fg); font: inherit; font-size: 14px; min-width: 0; }
  textarea { resize: vertical; }
  input:focus, textarea:focus { border-color: var(--accent); }
  .row { display: flex; gap: 8px; align-items: center; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 8px; padding: 6px 12px; color: var(--fg); }
  button:disabled { opacity: 0.45; }
  .primary { display: inline-flex; align-items: center; gap: 6px; background: var(--accent); border-color: var(--accent); color: var(--accent-fg); }
  .del { background: var(--del); border-color: var(--del); color: #fff; }
  .ghost { padding: 5px 10px; font-size: 13px; }
  .link { border: 0; background: none; padding: 0; color: var(--accent); text-decoration: underline; font: inherit; }
  .bar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .tabs { display: inline-flex; background: var(--hover); border-radius: 10px; padding: 3px; }
  .tabs button { border: 0; background: none; color: var(--muted); border-radius: 8px; padding: 5px 14px; }
  .tabs button.on { background: var(--surface); color: var(--fg); box-shadow: 0 1px 2px rgb(0 0 0 / 0.12); }
  .n { color: var(--muted); font-size: 12px; margin-left: 4px; }
  .auto { margin-left: auto; color: var(--muted); font-size: 12.5px; }
  .lockbtn { display: inline-flex; align-items: center; gap: 6px; }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; }
  .list li { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .list li:last-child { border-bottom: 0; }
  .list li.empty { color: var(--muted); justify-content: center; padding: 22px; text-align: center; font-size: 13.5px; }
  .main { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .main b { font-weight: 500; font-size: 14.5px; }
  .meta { color: var(--muted); font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta.val { white-space: pre-wrap; overflow-wrap: anywhere; }
  .acts { display: flex; align-items: center; gap: 2px; flex: none; }
  .icon { display: grid; place-items: center; border: 0; background: none; color: var(--muted); min-width: 30px; height: 30px; border-radius: 8px; padding: 0 6px; }
  .icon:hover:not(:disabled) { background: var(--hover); color: var(--fg); }
  .icon.txt { font-size: 12.5px; }
  .pick { flex-direction: row; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { border-radius: 999px; padding: 3px 12px; font-size: 13px; }
  .pw { display: flex; gap: 6px; }
  .pw input { flex: 1; font-family: ui-monospace, Consolas, monospace; }
  .meter { position: relative; height: 6px; border-radius: 3px; background: var(--hover); margin-top: -4px; }
  .meter span { display: block; height: 100%; border-radius: 3px; transition: width 0.3s, background 0.3s; background: var(--del); }
  .meter[data-level="2"] span { background: #d9a13a; }
  .meter[data-level="3"] span, .meter[data-level="4"] span { background: var(--add); }
  .meter small { position: absolute; right: 0; top: 9px; color: var(--muted); font-size: 12px; }
  .gen { display: grid; gap: 8px; border: 1px dashed var(--border); border-radius: 10px; padding: 10px 12px; }
  .range input { width: 100%; accent-color: var(--accent); }
  .checks { display: flex; flex-wrap: wrap; gap: 6px 16px; }
  .checks label { flex-direction: row; align-items: center; gap: 5px; color: var(--fg); }
  .how { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 4px; }
</style>
