/** Réglages de l'interface gardés par profil dans le stockage du navigateur (localStorage). */

export const PREFIX = "ollama-desktop.";
let namespace = "";
let inheritsLegacy = false;

/** Réglages de l'interface propres à un profil (clé API, thème, modèle…). Le profil issu de l'ancienne version reprend les anciens réglages. */
export function setProfile(id: string, legacy: boolean) {
  namespace = `p.${id}.`;
  inheritsLegacy = legacy;
}

/** Clé API éventuellement gardée en clair dans l'interface (ancienne version, ou repli si le coffre Windows est indisponible). */
export const peekStoredKey = () => load("webKey", "");

/** Efface cette clé locale une fois qu'elle est en sécurité dans le coffre Windows. */
export function clearStoredKey() {
  try {
    localStorage.removeItem(PREFIX + namespace + "webKey");
    if (inheritsLegacy) localStorage.removeItem(PREFIX + "webKey");
  } catch {}
}

export function load(key: string, fallback: string): string {
  try {
    return localStorage.getItem(PREFIX + namespace + key) ?? (inheritsLegacy ? localStorage.getItem(PREFIX + key) : null) ?? fallback;
  } catch {
    return fallback;
  }
}

export function save(key: string, value: string) {
  try {
    localStorage.setItem(PREFIX + namespace + key, value);
  } catch {}
}
