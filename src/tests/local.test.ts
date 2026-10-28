import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { ChatStore } from "$lib/chat.svelte";
import LocalModelsList from "$lib/LocalModelsList.svelte";
import ConfirmDialog from "$lib/ConfirmDialog.svelte";

type Installed = (typeof QWEN)[number];
const QWEN = [
  { id: "qwen3-8b", name: "Qwen3 8B", size_bytes: 5_027_783_488, tools: true, vision: false, license: "Apache-2.0", ctx: 32768, ctx_setting: 0, auto_ctx: 32768, max_ctx: 32768, options: [] },
];

/** Le faux backend répond comme si ces modèles étaient installés ; le reste vient du faux backend habituel. */
function withInstalled(list: Installed[]) {
  const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown, opts?: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
  const original = internals.invoke.bind(internals);
  let current = [...list];
  internals.invoke = (cmd, args, opts) => {
    if (cmd === "local_installed") return Promise.resolve(current);
    if (cmd === "local_remove") {
      const gone = current.find((m) => m.id === (args as { id: string }).id);
      current = current.filter((m) => m !== gone);
      return Promise.resolve(gone?.size_bytes ?? 0);
    }
    return original(cmd, args, opts);
  };
}

const hardware = { ram_mb: 24260, ram_free_mb: 14811, cpu: "AMD Ryzen 7", cores: 16, avx2: true, disk_free_mb: 69919, gpus: [] };

describe("file d'installation des modèles locaux", () => {
  it("met en file un second modèle sans lancer deux installations", async () => {
    const chat = new ChatStore("k");
    chat.localProgress = { id: "a", phase: "modèle", file: "a.gguf", done: 10, total: 100 };
    await chat.installLocal("b");
    await chat.installLocal("b"); // pas de doublon
    await chat.installLocal("a"); // celui qui s'installe n'est pas remis en file
    expect(chat.localQueue).toEqual(["b"]);
    chat.dequeueLocal("b");
    expect(chat.localQueue).toEqual([]);
  });

  it("annuler tout vide la file", async () => {
    const chat = new ChatStore("k");
    chat.localProgress = { id: "a", phase: "modèle", file: "a.gguf", done: 10, total: 100 };
    await chat.installLocal("b");
    await chat.installLocal("c");
    chat.cancelAllLocal();
    expect(chat.localQueue).toEqual([]);
  });

  describe("de bout en bout avec le faux backend", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    it("installe deux modèles l'un après l'autre, puis annonce le résultat", async () => {
      const chat = new ChatStore("k");
      chat.init();
      await vi.advanceTimersByTimeAsync(50);
      await chat.installLocal("qwen3-8b");
      await chat.installLocal("qwen3-14b");
      expect(chat.localQueue).toEqual(["qwen3-14b"]);
      await vi.advanceTimersByTimeAsync(8000);
      expect(chat.localInstalled.map((m) => m.id)).toEqual(["qwen3-8b"]);
      expect(chat.localProgress?.id).toBe("qwen3-14b");
      expect(chat.localQueue).toEqual([]);
      await vi.advanceTimersByTimeAsync(8000);
      expect(chat.localInstalled.map((m) => m.id)).toEqual(["qwen3-8b", "qwen3-14b"]);
      expect(chat.localProgress).toBeNull();
      expect(chat.localNotice).toContain("est installé");
    });

    it("une annulation n'est pas une erreur et la file continue", async () => {
      const chat = new ChatStore("k");
      chat.init();
      await vi.advanceTimersByTimeAsync(50);
      await chat.installLocal("qwen3-8b");
      await chat.installLocal("qwen3-14b");
      await vi.advanceTimersByTimeAsync(700);
      chat.cancelLocal("qwen3-8b");
      await vi.advanceTimersByTimeAsync(1500);
      expect(chat.localError).toBe("");
      expect(chat.localNotice).toContain("annulée");
      expect(chat.localProgress?.id).toBe("qwen3-14b");
    });
  });
});

describe("page des modèles locaux", () => {
  it("propose d'abord le modèle conseillé et affiche les filtres", async () => {
    const chat = new ChatStore("k");
    render(LocalModelsList, { chat, hardware });
    expect(await screen.findByText("Conseillé pour ton PC")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /À installer/ })).toHaveAttribute("aria-selected", "true");
    await fireEvent.click(screen.getByRole("button", { name: /^Images/ }));
    expect(await screen.findByText("Aucun modèle de cette catégorie à installer.")).toBeInTheDocument();
  });

  it("le bouton devient « Mettre en file » quand une installation est en cours", async () => {
    const chat = new ChatStore("k");
    render(LocalModelsList, { chat, hardware });
    await screen.findByText("Conseillé pour ton PC");
    chat.localProgress = { id: "ailleurs", phase: "modèle", file: "x", done: 1, total: 10 };
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Mettre en file" }).length).toBeGreaterThan(0));
  });

  it("supprime un modèle après confirmation, avec l'espace libéré", async () => {
    withInstalled(QWEN);
    const chat = new ChatStore("k");
    render(ConfirmDialog);
    render(LocalModelsList, { chat, hardware });
    await fireEvent.click(await screen.findByRole("button", { name: "Supprimer" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Ça libère 4,7 Go");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Supprimer" }));
    await waitFor(() => expect(chat.localNotice).toContain("Qwen3 8B supprimé"));
  });

  it("ne supprime rien si on refuse", async () => {
    withInstalled(QWEN);
    const chat = new ChatStore("k");
    render(ConfirmDialog);
    render(LocalModelsList, { chat, hardware });
    await fireEvent.click(await screen.findByRole("button", { name: "Supprimer" }));
    await fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Annuler" }));
    await waitFor(() => expect(chat.localInstalled).toHaveLength(1));
    expect(chat.localNotice).toBe("");
  });
});
