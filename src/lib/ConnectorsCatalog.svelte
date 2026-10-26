<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";

  type Field = { key: string; label: string; placeholder: string; help: string; secret: boolean; path: boolean; optional: boolean };
  type Connector = { id: string; name: string; desc: string; category: string; needs: string; command: string; fields: Field[]; setup: string; docs: string };
  type Status = { name: string; tools: string[]; error: string | null };

  let { servers, onreload }: { servers: Status[]; onreload: () => Promise<void> } = $props();

  let catalog = $state<Connector[]>([]);
  let installed = $state<string[]>([]);
  let openId = $state("");
  let values = $state<Record<string, string>>({});
  let busy = $state("");
  let message = $state<Record<string, string>>({});

  onMount(async () => {
    catalog = await invoke<Connector[]>("connectors_catalog");
    installed = await invoke<string[]>("connectors_installed");
  });

  // Serveurs MCP ajoutés à la main : ceux de mcp.json qui ne viennent pas du catalogue.
  const mine = $derived(installed.filter((id) => !catalog.some((c) => c.id === id)));
  let custom = $state({ open: false, tab: "form" as "form" | "paste", name: "", http: false, target: "", pairs: "", allSecret: false, text: "" });
  let customBusy = $state(false);
  let customMsg = $state("");

  async function addCustom() {
    customBusy = true;
    customMsg = "";
    try {
      const names =
        custom.tab === "form"
          ? [await invoke<string>("mcp_add_custom", { name: custom.name, http: custom.http, target: custom.target, pairs: custom.pairs, allSecret: custom.allSecret })]
          : await invoke<string[]>("mcp_import", { text: custom.text });
      installed = await invoke<string[]>("connectors_installed");
      await onreload();
      const report = names.map((n) => {
        const st = statusOf(n);
        return st?.error ? `${n} : ${st.error}` : `${n} : ${st?.tools.length ?? 0} outil${(st?.tools.length ?? 0) > 1 ? "s" : ""}`;
      });
      customMsg = `Ajouté — ${report.join(" · ")}`;
      custom = { open: false, tab: "form", name: "", http: false, target: "", pairs: "", allSecret: false, text: "" };
    } catch (e) {
      customMsg = String(e);
    } finally {
      customBusy = false;
    }
  }

  async function removeCustom(id: string) {
    customBusy = true;
    try {
      await invoke("mcp_remove", { name: id });
      installed = await invoke<string[]>("connectors_installed");
      await onreload();
      customMsg = `« ${id} » retiré.`;
    } catch (e) {
      customMsg = String(e);
    } finally {
      customBusy = false;
    }
  }

  const categories = $derived([...new Set(catalog.map((c) => c.category))]);
  const statusOf = (id: string) => servers.find((s) => s.name === id);

  function start(c: Connector) {
    openId = openId === c.id ? "" : c.id;
    values = {};
  }

  async function browse(f: Field) {
    const dir = f.key === "dir" || f.key === "repo";
    const sel = await openDialog({ directory: dir, multiple: false, title: f.label });
    if (typeof sel === "string") values[f.key] = sel;
  }

  async function add(c: Connector) {
    busy = c.id;
    message[c.id] = "";
    try {
      await invoke("connector_add", { id: c.id, values: $state.snapshot(values) });
      installed = await invoke<string[]>("connectors_installed");
      await onreload();
      const st = statusOf(c.id);
      message[c.id] = st?.error ? `Ajouté, mais le démarrage échoue : ${st.error}` : `Actif : ${st?.tools.length ?? 0} outil${(st?.tools.length ?? 0) > 1 ? "s" : ""} disponible${(st?.tools.length ?? 0) > 1 ? "s" : ""}.`;
      openId = "";
    } catch (e) {
      message[c.id] = String(e);
    } finally {
      busy = "";
    }
  }

  async function remove(c: Connector) {
    busy = c.id;
    try {
      await invoke("connector_remove", { id: c.id });
      installed = await invoke<string[]>("connectors_installed");
      await onreload();
      message[c.id] = "Retiré.";
    } catch (e) {
      message[c.id] = String(e);
    } finally {
      busy = "";
    }
  }
</script>

<section>
  <h3>Mes connecteurs</h3>
  <p class="muted">Ajoute n'importe quel serveur MCP : une commande lancée sur ce PC, l'adresse d'un serveur HTTP, ou une configuration copiée depuis une documentation (format Claude Desktop, Claude Code ou Cursor). Les clés et jetons vont dans le coffre Windows.</p>
  {#if customMsg}<p class="note" role="status">{customMsg}</p>{/if}
  <ul>
    {#each mine as id (id)}
      {@const st = statusOf(id)}
      <li>
        <div class="line">
          <span class="grow"><strong>{id}</strong>{#if st?.error}<br /><span class="err">{st.error}</span>{/if}</span>
          <span class="chip" class:bad={!!st?.error}>{st?.error ? "Erreur" : st ? `${st.tools.length} outil${st.tools.length > 1 ? "s" : ""}` : "Ajouté"}</span>
          <button onclick={() => removeCustom(id)} disabled={customBusy}>Retirer</button>
        </div>
      </li>
    {/each}
    <li class:open={custom.open}>
      <div class="line">
        <span class="grow"><strong>Serveur personnalisé</strong><br /><span class="muted">Une commande, une adresse HTTP, ou une configuration à coller.</span></span>
        <button onclick={() => (custom.open = !custom.open)}>{custom.open ? "Annuler" : "Ajouter"}</button>
      </div>
      {#if custom.open}
        <div class="tabs2" role="tablist">
          <button role="tab" aria-selected={custom.tab === "form"} class:on={custom.tab === "form"} onclick={() => (custom.tab = "form")}>Formulaire</button>
          <button role="tab" aria-selected={custom.tab === "paste"} class:on={custom.tab === "paste"} onclick={() => (custom.tab = "paste")}>Coller une configuration</button>
        </div>
        <form onsubmit={(e) => { e.preventDefault(); addCustom(); }}>
          {#if custom.tab === "form"}
            <label>
              <span>Nom</span>
              <input bind:value={custom.name} placeholder="Ex. mon-serveur" autocomplete="off" spellcheck="false" />
            </label>
            <div class="seg2" role="radiogroup" aria-label="Type de serveur">
              <button type="button" role="radio" aria-checked={!custom.http} class:on={!custom.http} onclick={() => (custom.http = false)}>Commande sur ce PC</button>
              <button type="button" role="radio" aria-checked={custom.http} class:on={custom.http} onclick={() => (custom.http = true)}>Adresse HTTP</button>
            </div>
            <label>
              <span>{custom.http ? "Adresse du serveur" : "Commande complète"}</span>
              <input bind:value={custom.target} placeholder={custom.http ? "http://127.0.0.1:8000/mcp" : "npx -y @scope/serveur-mcp --option"} autocomplete="off" spellcheck="false" />
              <small class="muted">{custom.http ? "Le serveur doit déjà être démarré." : "Mets les chemins avec des espaces entre guillemets. npx et uvx demandent Node.js ou uv."}</small>
            </label>
            <label>
              <span>{custom.http ? "En-têtes (facultatif)" : "Variables d'environnement (facultatif)"}</span>
              <textarea bind:value={custom.pairs} rows="3" spellcheck="false" placeholder={custom.http ? "Authorization: Bearer …" : "API_KEY=…"}></textarea>
              <small class="muted">Une par ligne. Les valeurs qui contiennent « key », « token », « secret », « password » ou « auth » vont dans le coffre.</small>
            </label>
            <label class="check"><input type="checkbox" bind:checked={custom.allSecret} /> Tout mettre dans le coffre, même les valeurs ordinaires</label>
          {:else}
            <label>
              <span>Configuration (JSON)</span>
              <textarea bind:value={custom.text} rows="8" spellcheck="false" placeholder={'{ "mcpServers": { "nom": { "command": "npx", "args": ["-y", "paquet"], "env": { "API_KEY": "…" } } } }'}></textarea>
              <small class="muted">Colle le bloc tel que la documentation du serveur le donne : plusieurs serveurs d'un coup sont acceptés.</small>
            </label>
          {/if}
          <div class="row">
            <button class="primary" disabled={customBusy || (custom.tab === "form" ? !custom.name.trim() || !custom.target.trim() : !custom.text.trim())}>
              {customBusy ? "Démarrage…" : custom.tab === "form" ? "Ajouter et tester" : "Importer et tester"}
            </button>
          </div>
        </form>
      {/if}
    </li>
  </ul>
</section>

<section>
  <h3>Catalogue de connecteurs</h3>
  <p class="muted">Choisis un service, remplis quelques champs et c'est fait. Les jetons vont dans le coffre Windows, jamais dans le fichier de configuration.
    Après l'ajout, Lama démarre le connecteur et affiche le nombre d'outils reçus, ou l'erreur. Ce sont des programmes tiers lancés sur ton PC avec <code>npx</code> ou <code>uvx</code> : il faut avoir Node.js ou uv installé.
    Je n'ai pas pu tous les essayer avec un vrai compte.</p>
  {#each categories as cat}
    <h4>{cat}</h4>
    <ul>
      {#each catalog.filter((c) => c.category === cat) as c (c.id)}
        {@const st = statusOf(c.id)}
        {@const on = installed.includes(c.id)}
        <li class:open={openId === c.id}>
          <div class="line">
            <span class="grow"><strong>{c.name}</strong><br /><span class="muted">{c.desc}</span></span>
            {#if on}
              <span class="chip" class:bad={!!st?.error}>{st?.error ? "Erreur" : st ? `${st.tools.length} outil${st.tools.length > 1 ? "s" : ""}` : "Ajouté"}</span>
              <button onclick={() => remove(c)} disabled={busy === c.id}>Retirer</button>
            {:else}
              <button onclick={() => start(c)}>{openId === c.id ? "Annuler" : "Ajouter"}</button>
            {/if}
          </div>
          {#if on && st?.error}<p class="err">{st.error}</p>{/if}
          {#if message[c.id]}<p class="note" role="status">{message[c.id]}</p>{/if}
          {#if openId === c.id}
            <form onsubmit={(e) => { e.preventDefault(); add(c); }}>
              {#if c.needs}<p class="muted">Nécessite {c.needs}.</p>{/if}
              {#if c.setup}<pre class="steps">{c.setup}</pre>{/if}
              {#each c.fields as f (f.key)}
                <label>
                  <span>{f.label}{#if f.optional} (facultatif){/if}</span>
                  <span class="inp">
                    <input type={f.secret ? "password" : "text"} bind:value={values[f.key]} placeholder={f.placeholder} autocomplete="off" spellcheck="false" />
                    {#if f.path}<button type="button" onclick={() => browse(f)}>Parcourir…</button>{/if}
                  </span>
                  {#if f.help}<small class="muted">{f.help}</small>{/if}
                </label>
              {/each}
              <div class="row">
                <button class="primary" disabled={busy === c.id}>{busy === c.id ? "Démarrage…" : "Ajouter et tester"}</button>
                {#if c.docs}<button type="button" class="link" onclick={() => openUrl(c.docs).catch(() => {})}>Documentation</button>{/if}
              </div>
            </form>
          {/if}
        </li>
      {/each}
    </ul>
  {/each}
</section>

<style>
  section { display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 0; font-size: 14px; }
  h4 { margin: 8px 0 0; font-size: 12.5px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; line-height: 1.45; }
  code { background: var(--code); padding: 1px 6px; border-radius: 5px; font-size: 12px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li { border: 1px solid var(--border); border-radius: 10px; padding: 8px 10px; background: var(--surface); display: flex; flex-direction: column; gap: 6px; }
  li.open { border-color: var(--accent); }
  .line { display: flex; align-items: center; gap: 10px; font-size: 14px; }
  .grow { flex: 1; min-width: 0; }
  .chip { font-size: 12px; padding: 1px 8px; border-radius: 6px; background: var(--hover); color: var(--add); white-space: nowrap; }
  .chip.bad { color: var(--del); }
  button { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 5px 12px; font-size: 13px; white-space: nowrap; }
  button:disabled { opacity: 0.5; }
  .err { color: var(--del); font-size: 12.5px; margin: 0; overflow-wrap: anywhere; }
  .note { color: var(--accent); font-size: 12.5px; margin: 0; }
  form { display: flex; flex-direction: column; gap: 8px; border-top: 1px solid var(--border); padding-top: 8px; }
  .steps { margin: 0; padding: 8px 10px; background: var(--code); border-radius: 8px; font-family: var(--sans); font-size: 12.5px; line-height: 1.55; white-space: pre-wrap; }
  label { display: flex; flex-direction: column; gap: 3px; font-size: 13px; }
  .inp { display: flex; gap: 6px; }
  input { flex: 1; min-width: 0; border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 6px 9px; outline: none; font-size: 13px; }
  input:focus { border-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 12px; }
  textarea { border: 1px solid var(--border); background: var(--bg); border-radius: 8px; padding: 6px 9px; outline: none; font-size: 13px; font-family: ui-monospace, Consolas, monospace; resize: vertical; color: var(--fg); }
  textarea:focus { border-color: var(--accent); }
  .check { flex-direction: row; align-items: center; gap: 8px; }
  .check input { flex: none; }
  .tabs2, .seg2 { display: flex; gap: 4px; }
  .tabs2 button, .seg2 button { border-radius: 8px; color: var(--muted); }
  .tabs2 button.on, .seg2 button.on { background: var(--hover); color: var(--fg); border-color: var(--accent); }
  .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .link { border: 0; background: none; color: var(--accent); text-decoration: underline; padding: 0; }
</style>
