import { invoke } from "@tauri-apps/api/core";
import { askConfirm } from "$lib/dialogs.svelte";

export type Draft = { mode: "file" | "dir" | "rename"; path: string; value: string };

/** État partagé de l'arbre de fichiers : saisie d'un nom (création, renommage) et actions sur le disque. */
export class FsCtl {
  draft = $state<Draft | null>(null);
  error = $state("");

  constructor(
    private workspace: () => string,
    private onchange: () => void,
  ) {}

  /** `path` : dossier parent pour une création, élément à renommer pour un renommage ("" = racine). */
  start(mode: Draft["mode"], path: string, value = "") {
    this.error = "";
    this.draft = { mode, path, value };
  }

  cancel() {
    this.draft = null;
    this.error = "";
  }

  async commit() {
    const d = this.draft;
    const name = d?.value.trim();
    if (!d || !name) return this.cancel();
    try {
      if (d.mode === "rename") {
        await invoke("fs_rename", { workspace: this.workspace(), rel: d.path, newName: name });
      } else {
        const rel = d.path ? `${d.path}/${name}` : name;
        await invoke("fs_create", { workspace: this.workspace(), rel, isDir: d.mode === "dir" });
      }
      this.cancel();
      this.onchange();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Envoie à la corbeille après confirmation. */
  async remove(path: string, isDir: boolean) {
    const what = isDir ? `le dossier « ${path} » et son contenu` : `« ${path} »`;
    if (!(await askConfirm(`Envoyer ${what} à la corbeille ?`, "Mettre à la corbeille"))) return;
    try {
      await invoke("fs_delete", { workspace: this.workspace(), rel: path });
      this.error = "";
      this.onchange();
    } catch (e) {
      this.error = String(e);
    }
  }
}
