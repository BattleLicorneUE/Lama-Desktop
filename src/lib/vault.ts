/** Coffre : types partagés avec le backend et estimation de la solidité d'un mot de passe. */

export type VaultStatus = { exists: boolean; unlocked: boolean; agent_access: boolean; logins: number; infos: number; locks_in: number; failures: number };
export type LoginView = { id: string; title: string; url: string; username: string; updated: number };
export type Login = { id: string; title: string; url: string; username: string; password: string; notes: string; created: number; updated: number };
export type Info = { id: string; label: string; value: string };
export type GenOpts = { length: number; upper: boolean; lower: boolean; digits: boolean; symbols: boolean; avoid_ambiguous: boolean };

export const DEFAULT_GEN: GenOpts = { length: 20, upper: true, lower: true, digits: true, symbols: true, avoid_ambiguous: true };

/** Rubriques proposées pour les informations rapides. */
export const INFO_PRESETS = ["Prénom", "Nom", "Nom complet", "E-mail", "Téléphone", "Adresse", "Complément d'adresse", "Code postal", "Ville", "Pays", "Date de naissance", "Société", "Site web"];

const COMMON = ["password", "motdepasse", "azerty", "qwerty", "123456", "111111", "abcdef", "letmein", "admin", "welcome", "iloveyou", "soleil", "bonjour"];

export type Strength = { bits: number; level: 0 | 1 | 2 | 3 | 4; label: string };

/** Estimation prudente : taille de l'alphabet utilisé × longueur, réduite pour les répétitions et les mots courants. */
export function strength(pw: string): Strength {
  if (!pw) return { bits: 0, level: 0, label: "—" };
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/[0-9]/.test(pw)) pool += 10;
  if (/[^A-Za-z0-9]/.test(pw)) pool += 32;
  const chars = [...pw];
  let bits = chars.length * Math.log2(Math.max(pool, 2));
  // Beaucoup de caractères identiques : moins d'information que la longueur le laisse croire.
  bits *= Math.min(1, (new Set(chars).size / chars.length) * 1.6);
  const low = pw.toLowerCase().replace(/[^a-z0-9]/g, "");
  // Un mot courant décoré (« Password123! ») se devine bien plus vite que sa longueur ne le laisse croire.
  if (COMMON.some((w) => low.includes(w))) bits = Math.min(bits - 18, 38);
  if (/(.)\1{2,}/.test(pw)) bits -= 8;
  if (/(0123|1234|2345|3456|4567|5678|6789|abcd|bcde|cdef)/i.test(pw)) bits -= 10;
  bits = Math.max(0, Math.round(bits));
  const level = bits < 28 ? 0 : bits < 40 ? 1 : bits < 60 ? 2 : bits < 80 ? 3 : 4;
  return { bits, level, label: ["Très faible", "Faible", "Moyen", "Bon", "Excellent"][level] };
}

/** « 4 min 12 s » */
export function countdown(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return s >= 60 ? `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s` : `${s} s`;
}
