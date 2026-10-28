import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { ChatStore } from "$lib/chat.svelte";
import SettingsDialog from "$lib/SettingsDialog.svelte";
import AnalyticsAsk from "$lib/AnalyticsAsk.svelte";
import { analytics } from "$lib/analytics.svelte";
import Page from "../routes/+page.svelte";

describe("parcours principal", () => {
  it("choisir un profil ouvre l'application", { timeout: 20000 }, async () => {
    localStorage.setItem("mock.secret", "demo-key"); // un compte connecté : la zone de saisie est proposée
    render(Page);
    const profile = await screen.findByRole("button", { name: /Principal/ }, { timeout: 4000 });
    await fireEvent.click(profile);
    // La barre latérale et la zone de saisie apparaissent une fois le profil ouvert.
    expect(await screen.findByRole("button", { name: /Nouveau/ }, { timeout: 6000 })).toBeInTheDocument();
    expect(await screen.findByLabelText("Ton message", {}, { timeout: 6000 })).toBeInTheDocument();
  });
});

describe("réglages", () => {
  const PAGES = [
    "Mon profil", "Mémoire", "Progression", "Apparence", "Préférences", "Ollama Cloud", "Modèles sur ce PC", "Choisir un modèle", "Recherche web",
    "Consommation", "Contrôle du PC", "Skills", "Agents", "Connecteurs", "Coffre", "Sauvegarde et effacement", "Statistiques anonymes", "Mises à jour",
  ];

  it("chaque page s'ouvre avec son titre et sa phrase d'explication", async () => {
    const chat = new ChatStore("k");
    render(SettingsDialog, { chat, profileName: "Principal", onclose: () => {} });
    for (const name of PAGES) {
      const nav = (await screen.findAllByRole("button", { name })).find((b) => b.closest("nav, aside, .nav, .side") || true)!;
      await fireEvent.click(nav);
      await waitFor(() => expect(screen.getAllByRole("heading", { name }).length).toBeGreaterThan(0), { timeout: 3000 });
    }
  });

  it("aucun bouton ni champ n'est sans nom (lecteurs d'écran)", async () => {
    const chat = new ChatStore("k");
    render(SettingsDialog, { chat, profileName: "Principal", onclose: () => {} });
    const nameOf = (el: Element) => {
      const aria = el.getAttribute("aria-label") || el.getAttribute("title");
      if (aria) return aria;
      if (el.getAttribute("aria-labelledby")) return "labelledby";
      if (el.closest("label")) return "label";
      if (el.id && document.querySelector(`label[for="${el.id}"]`)) return "label-for";
      if (el instanceof HTMLButtonElement) return el.textContent?.trim() ?? "";
      return "";
    };
    const unnamed: string[] = [];
    for (const name of PAGES) {
      await fireEvent.click((await screen.findAllByRole("button", { name }))[0]);
      await waitFor(() => expect(screen.getAllByRole("heading", { name }).length).toBeGreaterThan(0), { timeout: 3000 });
      await new Promise((r) => setTimeout(r, 30));
      for (const el of document.querySelectorAll("button, input, textarea, select")) {
        if (el instanceof HTMLInputElement && (el.type === "hidden" || el.type === "checkbox" || el.type === "radio")) continue;
        if (!nameOf(el)) unnamed.push(`${name} : <${el.tagName.toLowerCase()} class="${el.getAttribute("class") ?? ""}">`);
      }
    }
    expect(unnamed).toEqual([]);
  });

  it("la recherche trouve une page par ses mots-clés", async () => {
    const chat = new ChatStore("k");
    render(SettingsDialog, { chat, profileName: "Principal", onclose: () => {} });
    const search = screen.getByLabelText(/Rechercher/);
    await fireEvent.input(search, { target: { value: "mot de passe" } });
    expect(await screen.findByRole("button", { name: "Coffre" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Apparence" })).not.toBeInTheDocument();
    // Les accents et les majuscules ne comptent pas, l'ordre des mots non plus.
    await fireEvent.input(search, { target: { value: "THÈME sombre" } });
    expect(await screen.findByRole("button", { name: "Apparence" })).toBeInTheDocument();
    await fireEvent.input(search, { target: { value: "zzzz" } });
    expect(screen.queryByRole("button", { name: "Apparence" })).not.toBeInTheDocument();
  });
});

describe("statistiques : consentement", () => {
  it("la demande n'apparaît que si la question n'a jamais été posée, et enregistre le choix", async () => {
    render(AnalyticsAsk, { onmore: () => {} });
    const no = await screen.findByRole("button", { name: "Non merci" });
    expect(screen.getByText(/Jamais le contenu de tes conversations/)).toBeInTheDocument();
    await fireEvent.click(no);
    await waitFor(() => expect(analytics.info?.asked).toBe(true));
    expect(analytics.info?.enabled).toBe(false);
    expect(screen.queryByRole("dialog", { name: "Statistiques anonymes" })).not.toBeInTheDocument();
  });

  it("dire oui active l'envoi", async () => {
    render(AnalyticsAsk, { onmore: () => {} });
    await fireEvent.click(await screen.findByRole("button", { name: "Oui, d'accord" }));
    await waitFor(() => expect(analytics.info?.enabled).toBe(true));
    expect(within(document.body).queryByText("Non merci")).not.toBeInTheDocument();
  });
});
