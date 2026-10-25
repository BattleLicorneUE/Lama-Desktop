//! Texte des PDF joints à un message.

use std::path::Path;

const MAX_PDF: u64 = 40_000_000;
/// Au-delà, le texte est coupé : un PDF de 300 pages ne tiendrait de toute façon pas dans le contexte.
const MAX_CHARS: usize = 150_000;

/// Texte d'un PDF, une ligne vide entre les paragraphes. Les PDF scannés (images) n'ont pas de texte à lire.
pub fn extract_text(path: &Path) -> Result<String, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_PDF {
        return Err(format!("PDF trop volumineux ({} Mo max)", MAX_PDF / 1_000_000));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    // La bibliothèque peut paniquer sur un fichier mal formé : on ne laisse pas cela faire tomber l'application.
    let text = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes))
        .map_err(|_| format!("{name} : PDF illisible."))?
        .map_err(|e| format!("{name} : PDF illisible ({e})."))?;
    let text = tidy(&strip_repeated_lines(&text));
    if text.trim().is_empty() {
        return Err(format!("{name} ne contient pas de texte (PDF scanné ?) : joins plutôt des captures de ses pages si le modèle gère les images."));
    }
    if text.chars().count() > MAX_CHARS {
        let cut: String = text.chars().take(MAX_CHARS).collect();
        return Ok(format!("{cut}\n\n[… PDF tronqué : seuls les {MAX_CHARS} premiers caractères sont joints …]"));
    }
    Ok(text)
}

/// Numéro de page isolé (« 12 », « Page 3 », « 3 / 40 »).
fn is_page_number(t: &str) -> bool {
    let t = t.trim();
    let lower = t.to_lowercase();
    let t = lower.strip_prefix("page ").or_else(|| lower.strip_prefix("p. ")).unwrap_or(&lower);
    !t.is_empty() && t.len() <= 9 && t.chars().all(|c| c.is_ascii_digit() || matches!(c, ' ' | '/' | '-' | '.')) && t.chars().any(|c| c.is_ascii_digit())
}

/// Retire les en-têtes et pieds de page : lignes courtes qui reviennent au moins 4 fois, et numéros de page.
fn strip_repeated_lines(s: &str) -> String {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for l in s.lines().map(str::trim).filter(|l| !l.is_empty() && l.chars().count() <= 70) {
        *seen.entry(l).or_default() += 1;
    }
    s.lines()
        .filter(|l| {
            let t = l.trim();
            t.is_empty() || !(is_page_number(t) || seen.get(t).is_some_and(|&n| n >= 4))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Retire les espaces de fin et les longues séries de lignes vides.
pub fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_footers_and_page_numbers_are_dropped_but_content_stays() {
        let mut pdf = String::new();
        for page in 1..=5 {
            pdf.push_str(&format!("Rapport annuel 2026\nChapitre utile numéro {page}\nPage {page}\n{page} / 5\n"));
        }
        let out = tidy(&strip_repeated_lines(&pdf));
        assert!(!out.contains("Rapport annuel") && !out.contains("Page 3") && !out.contains("3 / 5"), "{out}");
        assert!(out.contains("Chapitre utile numéro 3"), "{out}");
        assert_eq!(tidy("a  \n\n\n\n\nb"), "a\n\nb");
    }

    /// PDF minimal à une page contenant « Bonjour PDF ».
    fn tiny_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            {
                let stream = "BT /F1 18 Tf 20 100 Td (Bonjour PDF) Tj ET";
                format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len())
            },
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = out.len();
        out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for off in offsets {
            out.push_str(&format!("{off:010} 00000 n \n"));
        }
        out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        out.into_bytes()
    }

    #[test]
    fn extracts_text_and_rejects_garbage() {
        let d = std::env::temp_dir().join(format!("pdf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let ok = d.join("a.pdf");
        std::fs::write(&ok, tiny_pdf()).unwrap();
        assert!(extract_text(&ok).unwrap().contains("Bonjour PDF"));
        let bad = d.join("b.pdf");
        std::fs::write(&bad, b"pas un pdf").unwrap();
        assert!(extract_text(&bad).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn tidy_collapses_blank_lines() {
        assert_eq!(tidy("a  \n\n\n\nb\n"), "a\n\nb");
    }
}
