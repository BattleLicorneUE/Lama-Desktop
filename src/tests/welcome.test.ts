import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { ChatStore, LOCAL_PREFIX } from "$lib/chat.svelte";
import Welcome from "$lib/Welcome.svelte";

describe("premier lancement", () => {
  it("propose le modèle conseillé pour ce PC, sans compte, à côté d'Ollama Cloud", async () => {
    render(Welcome, { chat: new ChatStore("") });
    expect(await screen.findByRole("button", { name: /Installer Qwen3 8B et commencer/ })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Ollama Cloud" })).toBeInTheDocument();
    expect(screen.getByLabelText("Clé API Ollama")).toBeInTheDocument();
    expect(screen.getByText(/Tient entièrement dans ta carte graphique/)).toBeInTheDocument();
  });

  describe("installation en un clic", () => {
    beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
    afterEach(() => vi.useRealTimers());

    it("installe le modèle, montre la progression, puis le choisit tout seul", async () => {
      const chat = new ChatStore("");
      chat.init();
      render(Welcome, { chat });
      await fireEvent.click(await screen.findByRole("button", { name: /Installer Qwen3 8B et commencer/ }));
      expect(await screen.findByRole("status")).toHaveTextContent(/Téléchargement|Préparation/);
      expect(screen.getByRole("button", { name: "Annuler" })).toBeInTheDocument();
      await vi.advanceTimersByTimeAsync(8000);
      await waitFor(() => expect(chat.localInstalled.map((m) => m.id)).toContain("qwen3-8b"));
      await waitFor(() => expect(chat.model).toBe(LOCAL_PREFIX + "qwen3-8b"));
    });
  });
});
