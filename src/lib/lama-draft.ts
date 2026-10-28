/** Rédaction assistée : on décrit une idée en une phrase, le modèle remplit le formulaire. */

/** Premier objet JSON trouvé dans une réponse, même entouré de texte ou de ```. */
export function parseJsonObject(text: string): Record<string, string> | null {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    const o = JSON.parse(text.slice(start, end + 1));
    if (typeof o !== "object" || !o || Array.isArray(o)) return null;
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(o)) out[k] = typeof v === "string" ? v : "";
    return out;
  } catch {
    return null;
  }
}

export const TASK_WRITER_PROMPT = `Tu aides à écrire une tâche planifiée pour un assistant IA qui l'exécutera seul, plus tard, sans pouvoir poser de question.
À partir de l'idée de l'utilisateur, écris la tâche en français.
Réponds uniquement avec un objet JSON valide, sans texte autour, de cette forme :
{"name":"...","prompt":"..."}
- "name" : un titre court (4 mots au plus).
- "prompt" : la consigne complète, à l'impératif : quoi faire ou chercher, avec quelles sources ou quels fichiers, le format du résultat attendu (liste, résumé de 5 lignes…), et quoi faire si une information manque. Concret et court.`;

export const PROJECT_WRITER_PROMPT = `Tu aides à écrire les instructions d'un projet pour un assistant IA : des consignes permanentes appliquées à toutes les conversations du projet.
À partir de l'idée de l'utilisateur, écris-les en français.
Réponds uniquement avec un objet JSON valide, sans texte autour, de cette forme :
{"name":"...","instructions":"..."}
- "name" : un nom de projet court.
- "instructions" : 5 à 10 lignes, à la deuxième personne : le contexte du projet, le ton à adopter, les conventions à respecter, ce qu'il faut éviter. Pour un projet de code, ajoute les commandes de test et de lancement si l'utilisateur les cite. N'invente pas de détails que l'utilisateur n'a pas donnés.`;
