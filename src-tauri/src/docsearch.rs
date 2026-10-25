//! Recherche dans les documents d'un projet : chaque document est découpé en passages, classés par mots (BM25).
//! Le modèle ne reçoit que les passages utiles au lieu des documents entiers.

use std::collections::HashMap;

/// Taille d'un passage et recouvrement avec le suivant, en caractères.
const CHUNK: usize = 1200;
const OVERLAP: usize = 150;
/// Les mots sont comparés sur leurs 6 premières lettres : « contrats », « contrat » et « contractuel » se rejoignent.
const STEM: usize = 6;

const STOP: &[&str] = &[
    "le", "la", "les", "un", "une", "des", "du", "de", "et", "ou", "en", "au", "aux", "ce", "ces", "se", "sa", "son", "ses", "que", "qui", "quoi", "dans", "sur", "par", "pour", "est", "sont",
    "the", "and", "for", "with", "that", "this", "are", "was", "pas", "plus", "avec", "elle", "il", "ils", "nous", "vous",
];

fn fold(c: char) -> char {
    match c {
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'à' | 'â' | 'ä' => 'a',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        _ => c,
    }
}

/// Mots significatifs d'un texte, en minuscules sans accents, tronqués.
fn terms(text: &str) -> Vec<String> {
    let flat: String = text.chars().flat_map(char::to_lowercase).map(fold).collect();
    flat.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2 && !STOP.contains(w))
        .map(|w| w.chars().take(STEM).collect())
        .collect()
}

/// Passages d'un texte : (position de départ en caractères, texte). Les coupes tombent de préférence sur un saut de ligne.
fn passages(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + CHUNK).min(chars.len());
        if end < chars.len() {
            if let Some(cut) = chars[start + CHUNK / 2..end].iter().rposition(|&c| c == '\n') {
                end = start + CHUNK / 2 + cut + 1;
            }
        }
        out.push((start, chars[start..end].iter().collect()));
        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP).max(start + 1);
    }
    out
}

/// Les `limit` passages les plus proches de la question, mis en forme pour le modèle.
pub fn search(docs: &[(String, String)], query: &str, limit: usize) -> String {
    let wanted = terms(query);
    let names = docs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
    if wanted.is_empty() {
        return format!("Question vide ou trop vague. Documents du projet : {names}");
    }
    struct Hit<'a> {
        doc: &'a str,
        start: usize,
        text: String,
        counts: HashMap<String, usize>,
        len: usize,
    }
    let mut hits: Vec<Hit> = Vec::new();
    for (name, content) in docs {
        for (start, text) in passages(content) {
            let words = terms(&text);
            let mut counts: HashMap<String, usize> = HashMap::new();
            for w in &words {
                if wanted.contains(w) {
                    *counts.entry(w.clone()).or_default() += 1;
                }
            }
            hits.push(Hit { doc: name, start, text, counts, len: words.len().max(1) });
        }
    }
    let n = hits.len() as f64;
    let avg = hits.iter().map(|h| h.len).sum::<usize>() as f64 / n.max(1.0);
    let mut scored: Vec<(f64, &Hit)> = hits
        .iter()
        .map(|h| {
            let score = wanted
                .iter()
                .map(|w| {
                    let tf = *h.counts.get(w).unwrap_or(&0) as f64;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let df = hits.iter().filter(|o| o.counts.contains_key(w)).count() as f64;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    idf * tf * 2.5 / (tf + 1.5 * (0.25 + 0.75 * h.len as f64 / avg))
                })
                .sum();
            (score, h)
        })
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    if scored.is_empty() {
        return format!("Aucun passage ne correspond à « {query} ». Documents du projet : {names}");
    }
    let found: Vec<String> = scored
        .iter()
        .take(limit.clamp(1, 8))
        .map(|(_, h)| format!("[{} — à partir du caractère {}]\n{}", h.doc, h.start, h.text.trim()))
        .collect();
    format!("{} passage(s) pour « {query} » :\n\n{}", found.len(), found.join("\n\n---\n\n"))
}

/// « il y a 3 jours » : âge lisible d'une date Unix.
fn ago(at: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let days = (now - at).max(0) / 86_400;
    match days {
        0 => "aujourd'hui".into(),
        1 => "hier".into(),
        2..=59 => format!("il y a {days} jours"),
        _ => format!("il y a {} mois", days / 30),
    }
}

/// Résultat de `search_chats` mis en forme pour le modèle : (titre, date, extrait).
pub fn chats_report(query: &str, found: &[(String, i64, String)]) -> String {
    if found.is_empty() {
        return format!("Aucune conversation passée ne parle de « {query} ».");
    }
    let lines: Vec<String> = found.iter().map(|(t, at, s)| format!("- « {t} » ({}) : {s}", ago(*at))).collect();
    format!("{} conversation(s) passée(s) pour « {query} » :\n{}", found.len(), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Vec<(String, String)> {
        let filler = "Texte neutre sans rapport avec la question posée. ".repeat(60);
        vec![
            ("contrat.md".into(), format!("{filler}\nLa résiliation du contrat exige un préavis de trois mois.\n{filler}")),
            ("menu.txt".into(), format!("{filler}\nLe gâteau au chocolat cuit quarante minutes.\n{filler}")),
        ]
    }

    #[test]
    fn the_right_passage_of_the_right_document_comes_first() {
        let r = search(&docs(), "Quel préavis pour résilier ?", 2);
        assert!(r.starts_with("2 passage(s)") || r.starts_with("1 passage(s)"), "{r}");
        assert!(r.contains("[contrat.md"), "{r}");
        assert!(r.contains("préavis de trois mois"), "{r}");
        let first = r.find("préavis de trois mois").unwrap();
        assert!(r.find("[menu.txt").is_none_or(|m| m > first));
    }

    #[test]
    fn accents_and_plurals_do_not_matter() {
        let r = search(&docs(), "gateaux chocolats", 1);
        assert!(r.contains("menu.txt") && r.contains("quarante minutes"), "{r}");
    }

    #[test]
    fn nothing_found_names_the_documents() {
        let r = search(&docs(), "astronomie", 3);
        assert!(r.contains("Aucun passage") && r.contains("contrat.md, menu.txt"), "{r}");
        assert!(search(&docs(), "le la de", 3).contains("trop vague"));
    }

    #[test]
    fn chat_reports_name_title_age_and_excerpt() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let r = chats_report("billet", &[("Train".into(), now - 3 * 86_400, "…un billet pour Lisbonne…".into())]);
        assert!(r.contains("« Train » (il y a 3 jours)") && r.contains("Lisbonne"), "{r}");
        assert!(chats_report("x", &[]).contains("Aucune conversation"));
    }

    #[test]
    fn long_texts_are_cut_in_overlapping_passages() {
        let text = "mot ".repeat(1000);
        let p = passages(&text);
        assert!(p.len() >= 3);
        assert!(p.windows(2).all(|w| w[1].0 < w[0].0 + w[0].1.chars().count()), "les passages doivent se chevaucher");
        assert!(passages("").is_empty());
    }
}
