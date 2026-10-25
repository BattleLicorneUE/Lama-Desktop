import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";

/** État partagé des mises à jour : la vérification au lancement, le bouton des réglages et la bannière lisent le même objet. */
class Updater {
  current = $state("");
  /** idle → checking → (none | available | error) ; available → installing. */
  status = $state<"idle" | "checking" | "none" | "available" | "installing" | "error">("idle");
  version = $state("");
  notes = $state("");
  error = $state("");
  /** Octets reçus / total pendant le téléchargement (total 0 = inconnu). */
  done = $state(0);
  total = $state(0);
  dismissed = $state(false);
  private update: Update | null = null;

  async check(silent = false) {
    if (this.status === "checking" || this.status === "installing") return;
    this.status = "checking";
    this.error = "";
    try {
      this.current ||= await getVersion();
      const found = await check();
      this.update = found;
      if (found) {
        this.version = found.version;
        this.notes = found.body ?? "";
        this.status = "available";
        this.dismissed = false;
      } else {
        this.status = "none";
      }
    } catch (e) {
      // Au lancement, une vérification qui échoue (hors ligne, dépôt privé…) ne dérange personne.
      this.status = silent ? "idle" : "error";
      this.error = String(e);
    }
  }

  /** Télécharge, vérifie la signature et installe ; Windows ferme l'application et la relance. */
  async install() {
    if (!this.update || this.status === "installing") return;
    this.status = "installing";
    this.done = 0;
    this.total = 0;
    try {
      await this.update.downloadAndInstall((ev) => {
        if (ev.event === "Started") this.total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") this.done += ev.data.chunkLength;
      });
    } catch (e) {
      this.status = "error";
      this.error = String(e);
    }
  }
}

export const updater = new Updater();
