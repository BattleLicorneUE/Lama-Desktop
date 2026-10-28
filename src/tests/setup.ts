import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";
import { cleanup } from "@testing-library/svelte";
import { installMock } from "$lib/dev-mock";
import { answerConfirm } from "$lib/dialogs.svelte";

// Le même faux backend que `npm run dev`, remis à neuf avant chaque test : aucun test ne dépend de ce que les autres ont installé ou supprimé.
beforeEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  localStorage.clear();
  installMock();
});
afterEach(() => {
  answerConfirm(false); // une confirmation restée ouverte ne doit pas passer au test suivant
  cleanup();
});
