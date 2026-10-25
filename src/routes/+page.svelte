<script lang="ts">
  import "../app.css";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import AppShell from "$lib/AppShell.svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import { ChatStore, clearStoredKey, peekStoredKey, setProfile } from "$lib/chat.svelte";
  import ProfilePicker from "$lib/ProfilePicker.svelte";
  import TitleBar from "$lib/TitleBar.svelte";
  import Logo from "$lib/Logo.svelte";
  import { fade } from "svelte/transition";
  import type { Profile } from "$lib/profiles";

  let profiles = $state<Profile[]>([]);
  let ready = $state(false);
  let error = $state("");
  /** Profil en cours d'ouverture (retour visuel immédiat au clic). */
  let opening = $state("");
  /** Profil ouvert et son état : chaque profil a son propre `ChatStore`, donc ses propres réglages et données. */
  let open = $state<{ profile: Profile; chat: ChatStore } | null>(null);
  /** Écran d'accueil animé à l'ouverture d'un profil : le lama et un bonjour, puis il s'efface. */
  let splash = $state<Profile | null>(null);
  let splashTimer: ReturnType<typeof setTimeout> | undefined;
  function showSplash(p: Profile) {
    clearTimeout(splashTimer);
    splash = p;
    splashTimer = setTimeout(() => (splash = null), 1500);
  }

  async function reload() {
    try {
      profiles = await invoke<Profile[]>("profiles_list");
    } catch (e) {
      error = String(e);
    }
    ready = true;
  }

  /** Clé API du profil : coffre Windows ; une ancienne clé en clair y est déplacée, puis effacée de l'interface. */
  async function loadKey(): Promise<string> {
    let key: string;
    try {
      key = (await invoke<string | null>("secret_get")) ?? "";
    } catch {
      return peekStoredKey(); // coffre inaccessible : on garde la clé locale telle quelle
    }
    if (!key) {
      key = peekStoredKey();
      if (key) {
        try {
          await invoke("secret_set", { value: key });
          clearStoredKey();
        } catch {}
      }
    }
    return key;
  }

  /** Ouvre un profil. Renvoie le message d'erreur (code PIN incorrect…), vide si le profil s'est ouvert. */
  async function enter(p: Profile, pin?: string): Promise<string> {
    if (opening) return "";
    error = "";
    opening = p.id;
    try {
      await invoke("profile_select", { id: p.id, pin: pin ?? null });
      setProfile(p.id, p.legacy);
      const chat = new ChatStore(await loadKey());
      chat.profileName = p.name;
      open = { profile: p, chat };
      showSplash(p);
      return "";
    } catch (e) {
      const message = String(e);
      // Un profil verrouillé affiche l'erreur dans sa fenêtre de code ; les autres sur l'écran principal.
      if (!p.locked) error = message;
      return message;
    } finally {
      opening = "";
    }
  }

  /** Dans une fenêtre supplémentaire, « changer de profil » ferme simplement la fenêtre. */
  function leave() {
    if (getCurrentWebviewWindow().label !== "main") {
      void getCurrentWebviewWindow().close();
      return;
    }
    open = null;
    reload();
  }

  /** Une fenêtre supplémentaire reprend le profil déjà ouvert (code PIN déjà saisi dans la fenêtre principale). */
  async function adopt() {
    await reload();
    try {
      const id = await invoke<string | null>("profile_active");
      const p = profiles.find((x) => x.id === id);
      if (!p) return;
      setProfile(p.id, p.legacy);
      const chat = new ChatStore(await loadKey());
      chat.profileName = p.name;
      open = { profile: p, chat };
      // Fenêtre ouverte depuis le tableau des agents : la tâche confiée démarre aussitôt.
      const task = await invoke<{ prompt: string; workspace: string | null; mode: string } | null>("window_task");
      if (task) void chat.startTask(task);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    if (getCurrentWebviewWindow().label === "main") reload();
    else adopt();
  });
</script>

<div class="frame">
  <TitleBar />
  <div class="content">
    {#if open}
      {#key open.profile.id}
        <AppShell chat={open.chat} profile={open.profile} onswitch={leave} />
      {/key}
    {:else if ready}
      <ProfilePicker {profiles} {error} {opening} onopen={enter} onchange={reload} />
    {/if}
  </div>
</div>
{#if splash}
  <div class="splash" out:fade={{ duration: 450 }} aria-hidden="true">
    <Logo size={96} animated />
    <p class="hello">Bonjour {splash.name}</p>
    <span class="bar"></span>
  </div>
{/if}
<ConfirmDialog />

<style>
  .frame { display: flex; flex-direction: column; height: 100vh; }
  .content { flex: 1; min-height: 0; overflow: auto; }
  .splash { position: fixed; inset: 0; z-index: 200; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 18px; background: var(--bg); pointer-events: none; }
  .hello { margin: 0; font-family: var(--serif); font-size: 28px; animation: rise 0.6s 0.35s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .bar { width: 120px; height: 3px; border-radius: 2px; background: var(--hover); overflow: hidden; position: relative; animation: fadein 0.4s 0.5s both; }
  .bar::after { content: ""; position: absolute; inset: 0; width: 40%; border-radius: 2px; background: var(--accent); animation: slide 1s 0.5s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
</style>
