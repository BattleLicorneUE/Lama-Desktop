export type Profile = { id: string; name: string; color: string; legacy: boolean; locked: boolean };

/** Couleurs proposées pour l'avatar (les mêmes que côté Rust). */
export const AVATAR_COLORS = ["#c96442", "#3a78e0", "#2fa876", "#b08a3a", "#7c4ddb", "#d4507a", "#2a9d9d", "#6b7280"];

export const initialOf = (name: string) => (name.trim()[0] ?? "?").toUpperCase();
