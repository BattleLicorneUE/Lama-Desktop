import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { ChatStore } from "$lib/chat.svelte";
import { searchUrl, toAddress, urlForTool } from "$lib/browser";
import BrowserPane from "$lib/BrowserPane.svelte";

describe("adresses du navigateur", () => {
  it("la barre accepte une adresse, un nom de site ou une recherche", () => {
    expect(toAddress("https://example.org/a?b=1")).toBe("https://example.org/a?b=1");
    expect(toAddress("example.org/page")).toBe("https://example.org/page");
    expect(toAddress("  lama desktop ollama ")).toBe(searchUrl("lama desktop ollama"));
    expect(toAddress("c'est quoi un lama ?")).toBe(searchUrl("c'est quoi un lama ?"));
    expect(toAddress("")).toBe("https://duckduckgo.com/");
  });

  it("une adresse locale n'est jamais ajoutée devant : elle part en recherche, le cœur Rust la refuse de toute façon", () => {
    expect(toAddress("javascript:alert(1)")).toBe(searchUrl("javascript:alert(1)"));
    expect(toAddress("file:///C:/Windows/win.ini")).toBe(searchUrl("file:///C:/Windows/win.ini"));
  });

  it("Lama montre sa recherche et la page qu'il lit, rien d'autre", () => {
    expect(urlForTool("web_search", { query: "météo à Lyon" })).toBe("https://duckduckgo.com/?q=m%C3%A9t%C3%A9o%20%C3%A0%20Lyon");
    expect(urlForTool("web_search", { query: "  " })).toBeNull();
    expect(urlForTool("web_fetch", { url: "https://example.org/x" })).toBe("https://example.org/x");
    expect(urlForTool("web_fetch", { url: "file:///etc/passwd" })).toBeNull();
    expect(urlForTool("read_file", { path: "a.txt" })).toBeNull();
  });
});

describe("navigateur intégré", () => {
  it("s'ouvre pour une recherche de Lama, une seule fois par appel, et se règle", () => {
    const chat = new ChatStore("t");
    expect(chat.browserAuto).toBe(true);
    chat.openBrowser("https://example.org/");
    expect(chat.browser?.url).toBe("https://example.org/");
    const n = chat.browser!.nonce;
    chat.openBrowser("https://example.org/b");
    expect(chat.browser!.nonce).toBe(n + 1);
    chat.closeBrowser();
    expect(chat.browser).toBeNull();
    chat.setBrowserAuto(false);
    expect(new ChatStore("t").browserAuto).toBe(false);
  });

  it("le panneau a une barre d'adresse, des boutons nommés et affiche l'erreur du cœur", async () => {
    render(BrowserPane, { url: "https://example.org/", nonce: 1, onclose: () => {} });
    for (const name of ["Page précédente", "Page suivante", "Ouvrir dans le navigateur par défaut", "Fermer le navigateur"]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
    const address = screen.getByLabelText("Adresse ou recherche") as HTMLInputElement;
    await waitFor(() => expect(address.value).toBe("https://example.org/"));
    await fireEvent.input(address, { target: { value: "lama" } });
    expect(address.value).toBe("lama");
  });
});
