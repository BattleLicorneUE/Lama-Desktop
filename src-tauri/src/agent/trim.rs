//! Allègement de la conversation quand le contexte se remplit : on réduit d'abord les anciennes sorties d'outils.

use super::*;

/// Part du contexte à partir de laquelle les anciens résultats d'outils sont retirés de l'historique.
pub(super) const TRIM_AT: f64 = 0.8;

/// Résultats d'outils les plus récents conservés intacts ; moins quand le contexte reste très plein.
pub(super) const KEEP_RECENT: usize = 6;

pub(super) const KEEP_RECENT_TIGHT: usize = 2;

/// En dessous de cette taille, un résultat ancien n'est pas remplacé (le gain serait négligeable).
pub(super) const TRIM_MIN_CHARS: usize = 400;

/// Estimation grossière des jetons de l'historique et des définitions d'outils (≈ 3,5 caractères par jeton).
pub(super) fn estimate_tokens(msgs: &[ChatMessage], defs: &[Value]) -> usize {
    let chars: usize = msgs.iter().map(|m| m.content.len() + m.tool_calls.as_ref().map_or(0, |c| c.iter().map(|v| v.to_string().len()).sum())).sum::<usize>()
        + defs.iter().map(|d| d.to_string().len()).sum::<usize>();
    (chars as f64 / 3.5) as usize
}

/// Remplace les anciens résultats d'outils (hors les `keep` derniers) par un court rappel. Renvoie le nombre remplacé.
pub(super) fn trim_tool_outputs(msgs: &mut [ChatMessage], keep: usize) -> usize {
    let tool_idx: Vec<usize> = msgs.iter().enumerate().filter(|(_, m)| m.role == "tool").map(|(i, _)| i).collect();
    let old = tool_idx.len().saturating_sub(keep);
    let mut trimmed = 0;
    for &i in &tool_idx[..old] {
        let m = &mut msgs[i];
        if m.content.len() > TRIM_MIN_CHARS && !m.content.starts_with("[résultat") {
            m.content = format!(
                "[résultat de {} retiré pour libérer du contexte ({} caractères) : relance l'outil si tu en as encore besoin]",
                m.tool_name.as_deref().unwrap_or("l'outil"),
                m.content.len()
            );
            trimmed += 1;
        }
    }
    trimmed
}
