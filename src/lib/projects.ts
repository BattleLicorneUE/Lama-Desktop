/** Première ligne utile des instructions d'un projet : sert de description sur les cartes et en sous-titre. */
export function projectBlurb(instructions: string): string {
  const line = instructions
    .split("\n")
    .map((l) => l.replace(/^[#>*\-\s]+/, "").trim())
    .find(Boolean);
  return line ?? "Aucune instruction : ajoute-en pour guider les chats de ce projet.";
}

/** « 11 oct. » dans l'année en cours, « 13 nov. 2025 » sinon. */
export function shortDate(seconds: number, now = new Date()): string {
  if (!seconds) return "";
  const d = new Date(seconds * 1000);
  return d.toLocaleDateString("fr-FR", { day: "numeric", month: "short", ...(d.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }) });
}
