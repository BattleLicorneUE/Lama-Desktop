export type MenuItem = { id: string; label: string; desc?: string; badge?: string; accent?: boolean };

export const PERMISSION_MODES: MenuItem[] = [
  { id: "auto", label: "Auto", badge: "Recommandé", desc: "L'agent gère les décisions d'autorisation" },
  { id: "manual", label: "Manuel", desc: "Toujours demander avant d'apporter des modifications" },
  { id: "accept-edits", label: "Accepter les modifications", desc: "Accepter toutes les modifications auto." },
  { id: "plan", label: "Plan", desc: "Créer un plan avant d'apporter des modifications" },
  { id: "bypass", label: "Ignorer les permissions", desc: "Accepte toutes les autorisations" },
];

export const MODE_IDS = PERMISSION_MODES.map((m) => m.id);

/** Consigne du mode Plan, aussi utilisée en Chat (où l'effort ne se règle pas). */
export const PLAN_HINT = "Avant toute modification, présente d'abord un plan détaillé par étapes et attends la validation de l'utilisateur.";

export const EFFORTS: MenuItem[] = [
  { id: "low", label: "Faible" },
  { id: "medium", label: "Moyen", badge: "Recommandé" },
  { id: "high", label: "Élevé" },
  { id: "extra", label: "Extra" },
  { id: "max", label: "Max" },
  { id: "ultracode", label: "Ultracode", accent: true },
];

const EFFORT_HINT: Record<string, string> = {
  low: "Réponds de façon brève et directe, sans détour.",
  medium: "",
  high: "Réfléchis soigneusement et vérifie ton raisonnement avant de répondre.",
  extra: "Analyse le problème en profondeur, envisage les cas limites et justifie tes choix.",
  max: "Fais une analyse exhaustive : explore les alternatives, vérifie chaque étape et sois rigoureux.",
  ultracode: "Travaille comme un ingénieur senior : analyse exhaustive, code complet et testé mentalement, cas limites traités.",
};

const MODE_HINT: Record<string, string> = {
  plan: PLAN_HINT,
};

/** Consigne système dérivée des réglages du mode Code (le contexte, lui, vient du modèle). */
export function codeRequest(effort: string, permission: string) {
  const system = [EFFORT_HINT[effort], MODE_HINT[permission]].filter(Boolean).join("\n");
  return { options: {}, system };
}
