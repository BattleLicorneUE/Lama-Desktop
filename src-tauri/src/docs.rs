//! Markdown → page HTML soignée (titre, sommaire, sections numérotées, vrais tableaux, code, citations).
//! La même page sert à l'export HTML et, imprimée par le moteur de rendu de l'application, à l'export PDF.

use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Default, Clone)]
pub struct DocOpts {
    pub subtitle: Option<String>,
    /// `None` : sommaire automatique à partir de quatre titres.
    pub toc: Option<bool>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn plain_text(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            Event::SoftBreak | Event::HardBreak => Some(" "),
            _ => None,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn slug(text: &str, used: &mut Vec<String>) -> String {
    let mut s: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() {
        s = "section".into();
    }
    let base = s.clone();
    let mut n = 2;
    while used.contains(&s) {
        s = format!("{base}-{n}");
        n += 1;
    }
    used.push(s.clone());
    s
}

/// Deux titres sont « les mêmes » s'ils ne diffèrent que par la casse ou la ponctuation.
fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    norm(a) == norm(b)
}

fn starts_numbered(t: &str) -> bool {
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && t.chars().nth(digits).is_some_and(|c| c == '.' || c == ')')
}

const STYLE: &str = r#"
:root { --accent: #c96442; --ink: #1f2328; --muted: #6b6760; --line: #e4e1d8; --soft: #f6f5f2; }
* { -webkit-print-color-adjust: exact; print-color-adjust: exact; box-sizing: border-box; }
body { margin: 0 auto; max-width: 820px; padding: 28px 24px; font: 10.5pt/1.6 "Segoe UI", Calibri, "Helvetica Neue", Arial, sans-serif; color: var(--ink); }
@media print { body { max-width: none; padding: 0; } }
.cover { margin: 0 0 22px; padding-bottom: 14px; border-bottom: 3px solid var(--accent); }
.cover .doc-title { margin: 0; font-size: 25pt; line-height: 1.15; letter-spacing: -0.01em; border: 0; padding: 0; }
.cover .subtitle { margin: 8px 0 0; font-size: 12.5pt; color: var(--muted); }
.toc { margin: 0 0 24px; padding: 14px 18px; background: var(--soft); border: 1px solid var(--line); border-radius: 8px; break-inside: avoid; }
.toc .toc-title { margin: 0 0 8px; font-size: 11pt; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--accent); }
.toc ol { margin: 0; padding-left: 0; list-style: none; }
.toc li { margin: 3px 0; }
.toc li.sub { margin-left: 22px; font-size: 9.8pt; color: var(--muted); }
.toc a { color: inherit; text-decoration: none; }
.toc .n { display: inline-block; min-width: 24px; color: var(--accent); font-weight: 600; }
h1, h2, h3, h4 { line-height: 1.25; break-after: avoid; break-inside: avoid; }
h1 { font-size: 18pt; margin: 26px 0 8px; }
h2 { font-size: 15pt; margin: 26px 0 8px; padding-bottom: 5px; border-bottom: 1px solid var(--line); }
h3 { font-size: 12.5pt; margin: 18px 0 6px; }
h4 { font-size: 11pt; margin: 14px 0 4px; color: var(--muted); }
body.numbered { counter-reset: s1 s2; }
body.numbered h2 { counter-increment: s1; counter-reset: s2; }
body.numbered h2::before { content: counter(s1) ".  "; color: var(--accent); }
body.numbered h3 { counter-increment: s2; }
body.numbered h3::before { content: counter(s1) "." counter(s2) "  "; color: var(--muted); font-weight: 600; }
p { margin: 0 0 9px; orphans: 3; widows: 3; }
ul, ol { margin: 0 0 10px; padding-left: 22px; }
li { margin: 2px 0; }
li > p { margin: 0; }
a { color: #0b5cad; }
strong { font-weight: 700; }
hr { border: 0; border-top: 1px solid var(--line); margin: 18px 0; }
blockquote { margin: 10px 0 12px; padding: 8px 14px; border-left: 4px solid var(--accent); background: #fbf3ef; border-radius: 0 6px 6px 0; color: #3a3633; break-inside: avoid; }
blockquote p:last-child { margin-bottom: 0; }
code { font: 9pt Consolas, "Cascadia Mono", monospace; background: var(--soft); border: 1px solid var(--line); border-radius: 4px; padding: 0 4px; }
pre { margin: 10px 0 12px; padding: 9px 12px; background: var(--soft); border: 1px solid var(--line); border-radius: 6px; font: 8.8pt/1.5 Consolas, "Cascadia Mono", monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
pre code { background: none; border: 0; padding: 0; font: inherit; }
table { width: 100%; border-collapse: collapse; margin: 10px 0 14px; font-size: 9.5pt; }
thead { display: table-header-group; }
tr { break-inside: avoid; }
th { background: #2b2a27; color: #fff; text-align: left; padding: 6px 9px; font-weight: 600; }
td { padding: 5px 9px; border-bottom: 1px solid var(--line); vertical-align: top; }
tbody tr:nth-child(even) td { background: #faf9f6; }
img { max-width: 100%; height: auto; }
input[type=checkbox] { margin-right: 6px; }
.footnote-definition { font-size: 9pt; color: var(--muted); }
"#;

/// Page HTML complète. Le contenu brut (HTML, scripts) du Markdown est affiché comme du texte, jamais exécuté.
pub fn document_html(title: &str, md: &str, opts: &DocOpts) -> String {
    let parser_opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_FOOTNOTES;
    let mut events: Vec<Event> = Parser::new_ext(md, parser_opts)
        .map(|e| match e {
            Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
            other => other,
        })
        .collect();

    // Le premier titre de niveau 1 est le titre du document : il passe en page de garde, pas dans le corps.
    let mut doc_title = title.trim().to_string();
    let mut subtitle = opts.subtitle.clone().filter(|s| !s.trim().is_empty());
    let first_heading = events.iter().position(|e| matches!(e, Event::Start(Tag::Heading { .. })));
    if let Some(start) = first_heading {
        if matches!(&events[start], Event::Start(Tag::Heading { level: HeadingLevel::H1, .. })) {
            if let Some(end) = events[start..].iter().position(|e| matches!(e, Event::End(TagEnd::Heading(_)))).map(|i| i + start) {
                let text = plain_text(&events[start + 1..end]);
                events.drain(start..=end);
                if doc_title.is_empty() {
                    doc_title = text;
                } else if !same(&text, &doc_title) && subtitle.is_none() && !text.is_empty() {
                    subtitle = Some(text);
                }
            }
        }
    }
    if doc_title.is_empty() {
        doc_title = "Document".into();
    }

    // Identifiants de titres (liens du sommaire).
    let mut heads: Vec<(u8, String, String)> = Vec::new();
    let mut used = Vec::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let lvl = *level as u8;
            let end = events[i..].iter().position(|e| matches!(e, Event::End(TagEnd::Heading(_)))).map_or(events.len(), |p| p + i);
            let text = plain_text(&events[i + 1..end]);
            let id = slug(&text, &mut used);
            if let Event::Start(Tag::Heading { id: slot, .. }) = &mut events[i] {
                *slot = Some(CowStr::from(id.clone()));
            }
            heads.push((lvl, text, id));
        }
        i += 1;
    }

    let numbered = !heads.iter().any(|(_, t, _)| starts_numbered(t));
    let entries: Vec<&(u8, String, String)> = heads.iter().filter(|(l, _, _)| *l <= 3).collect();
    let show_toc = opts.toc.unwrap_or(entries.len() >= 4) && !entries.is_empty();

    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());

    let mut toc = String::new();
    if show_toc {
        toc.push_str("<nav class=\"toc\"><div class=\"toc-title\">Sommaire</div><ol>");
        let (mut s1, mut s2) = (0, 0);
        for (lvl, text, id) in entries {
            let sub = *lvl >= 3;
            let number = if !numbered {
                String::new()
            } else if sub {
                s2 += 1;
                format!("{s1}.{s2}")
            } else {
                s1 += 1;
                s2 = 0;
                format!("{s1}.")
            };
            toc.push_str(&format!("<li class=\"{}\"><a href=\"#{}\"><span class=\"n\">{}</span>{}</a></li>", if sub { "sub" } else { "top" }, esc(id), number, esc(text)));
        }
        toc.push_str("</ol></nav>");
    }

    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data: https:\"><title>{t}</title><style>{STYLE}</style></head><body class=\"{cls}\"><header class=\"cover\"><h1 class=\"doc-title\">{t}</h1>{sub}</header>{toc}{body}</body></html>",
        t = esc(&doc_title),
        cls = if numbered { "numbered" } else { "" },
        sub = subtitle.map(|s| format!("<p class=\"subtitle\">{}</p>", esc(&s))).unwrap_or_default(),
    )
}

/// Page HTML fournie telle quelle (artefact) : scripts interdits, styles et images courants autorisés.
pub fn static_page(html: &str) -> String {
    let guard = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline' https:; img-src data: https:; font-src data: https:\">";
    let lower = html.to_lowercase();
    if let Some(i) = lower.find("<head") {
        if let Some(j) = lower[i..].find('>') {
            let at = i + j + 1;
            return format!("{}{guard}{}", &html[..at], &html[at..]);
        }
    }
    format!("<!doctype html><html><head><meta charset=\"utf-8\">{guard}</head><body>{html}</body></html>")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# Rapport trimestriel\n\n> Résumé : les ventes progressent de **12 %**.\n\n## Contexte\n\nTexte avec `code` et un [lien](https://exemple.fr).\n\n## Résultats\n\n| Région | Ventes | Évolution |\n|---|--:|---|\n| Nord | 120 | +8 % |\n| Sud | 98 | +15 % |\n\n### Détails\n\n- point un\n- point deux\n\n## Conclusion\n\nFin.\n\n<script>alert(1)</script>\n";

    #[test]
    fn builds_cover_toc_tables_and_numbering() {
        let h = document_html("", MD, &DocOpts::default());
        assert!(h.contains("<h1 class=\"doc-title\">Rapport trimestriel</h1>"), "titre repris du premier H1");
        assert_eq!(h.matches("Rapport trimestriel").count(), 2, "le H1 n'est pas répété dans le corps (titre + <title>)");
        assert!(h.contains("<nav class=\"toc\">") && h.contains("href=\"#contexte\"") && h.contains("href=\"#détails\""), "sommaire avec liens");
        assert!(h.contains("<h2 id=\"résultats\">") && h.contains("<table>") && h.contains("<th"), "ids et vrais tableaux");
        assert!(h.contains("<blockquote>") && h.contains("<strong>12 %</strong>"));
        assert!(h.contains("class=\"numbered\""), "sections numérotées");
        assert!(h.contains("&lt;script&gt;") && !h.contains("<script>"), "le HTML brut n'est jamais exécutable");
    }

    #[test]
    fn skips_numbering_when_titles_are_already_numbered_and_toc_is_optional() {
        let md = "## 1. Intro\n\ntexte\n\n## 2. Suite\n\ntexte\n";
        let h = document_html("Mon doc", md, &DocOpts::default());
        assert!(!h.contains("class=\"numbered\"") && !h.contains("<nav class=\"toc\">"), "pas de sommaire sous quatre titres");
        let forced = document_html("Mon doc", md, &DocOpts { toc: Some(true), subtitle: Some("Version 2".into()) });
        assert!(forced.contains("<nav class=\"toc\">") && forced.contains("Version 2"));
        let off = document_html("Mon doc", MD, &DocOpts { toc: Some(false), ..Default::default() });
        assert!(!off.contains("<nav class=\"toc\">"));
    }

    #[test]
    fn different_first_h1_becomes_subtitle_and_duplicate_ids_are_unique() {
        let h = document_html("Bilan 2026", "# Synthèse annuelle\n\n## A\n\n## A\n", &DocOpts::default());
        assert!(h.contains("<p class=\"subtitle\">Synthèse annuelle</p>") && h.contains("id=\"a\"") && h.contains("id=\"a-2\""));
    }

    #[test]
    fn static_pages_block_scripts() {
        let p = static_page("<html><head><title>x</title></head><body><p>hi</p></body></html>");
        assert!(p.contains("default-src 'none'") && p.contains("<p>hi</p>"));
        assert!(static_page("<p>fragment</p>").contains("<body><p>fragment</p></body>"));
    }
}
