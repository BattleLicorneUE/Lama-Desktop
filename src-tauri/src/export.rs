//! Export de documents (PDF, Word, HTML, Markdown) à partir de texte Markdown : conversations, réponses, documents créés par le modèle.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufWriter, Cursor, Write};
use std::path::{Path, PathBuf};

use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    /// Puce ou numéro (« • », « 1. »), puis le texte.
    Item(String, String),
    /// Bloc de code ou tableau : lignes à largeur fixe.
    Pre(Vec<String>),
    Rule,
}

/// Retire les marques Markdown en ligne : gras, italique, code, liens.
fn plain(s: &str) -> String {
    let link = Regex::new(r"!?\[([^\]]*)\]\(([^)]*)\)").unwrap();
    let s = link.replace_all(s, |c: &regex::Captures| if c[1].is_empty() { c[2].to_string() } else { c[1].to_string() });
    s.replace("**", "").replace("__", "").replace('`', "").trim_start_matches("> ").to_string()
}

pub fn parse(md: &str) -> Vec<Block> {
    let numbered = Regex::new(r"^(\d{1,3})[.)]\s+(.*)$").unwrap();
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    let mut in_table = false;
    let flush = |para: &mut Vec<String>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(plain(&para.join(" "))));
            para.clear();
        }
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(c) => out.push(Block::Pre(c)),
                None => {
                    flush(&mut para, &mut out);
                    code = Some(Vec::new());
                }
            }
            continue;
        }
        if let Some(c) = code.as_mut() {
            c.push(line.replace('\t', "    "));
            continue;
        }
        let t = line.trim();
        let table = t.starts_with('|');
        if !table {
            in_table = false;
        }
        if t.is_empty() {
            flush(&mut para, &mut out);
        } else if table {
            flush(&mut para, &mut out);
            if in_table {
                if let Some(Block::Pre(rows)) = out.last_mut() {
                    rows.push(t.to_string());
                }
            } else {
                out.push(Block::Pre(vec![t.to_string()]));
                in_table = true;
            }
        } else if t.starts_with('#') {
            flush(&mut para, &mut out);
            let level = t.chars().take_while(|c| *c == '#').count().min(3) as u8;
            out.push(Block::Heading(level, plain(t.trim_start_matches('#').trim())));
        } else if t == "---" || t == "***" || t == "___" {
            flush(&mut para, &mut out);
            out.push(Block::Rule);
        } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
            flush(&mut para, &mut out);
            out.push(Block::Item("•".into(), plain(rest)));
        } else if let Some(c) = numbered.captures(t) {
            flush(&mut para, &mut out);
            out.push(Block::Item(format!("{}.", &c[1]), plain(&c[2])));
        } else if t.starts_with('<') && t.ends_with('>') {
            // Balises HTML isolées (<details>…) : sans intérêt dans un document.
        } else {
            para.push(t.to_string());
        }
    }
    if let Some(c) = code {
        out.push(Block::Pre(c));
    }
    flush(&mut para, &mut out);
    out
}

// ---------------------------------------------------------------- PDF

struct Font {
    reference: printpdf::IndirectFontRef,
    bytes: Vec<u8>,
    advances: RefCell<HashMap<char, f32>>,
}

impl Font {
    /// Largeur du caractère en points, pour une taille de 1 pt.
    fn unit(&self, c: char) -> f32 {
        if let Some(w) = self.advances.borrow().get(&c) {
            return *w;
        }
        let w = ttf_parser::Face::parse(&self.bytes, 0)
            .ok()
            .and_then(|f| {
                let upem = f32::from(f.units_per_em());
                f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).map(|a| f32::from(a) / upem)
            })
            .unwrap_or(0.55);
        self.advances.borrow_mut().insert(c, w);
        w
    }

    fn width_mm(&self, s: &str, size: f32) -> f32 {
        s.chars().map(|c| self.unit(c)).sum::<f32>() * size * 0.3528
    }
}

fn font_file(names: &[&str]) -> Result<Vec<u8>, String> {
    let dir = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    names
        .iter()
        .find_map(|n| std::fs::read(dir.join(n)).ok())
        .ok_or_else(|| format!("Police introuvable dans {} ({})", dir.display(), names.join(", ")))
}

/// Coupe le texte en lignes qui tiennent dans `max` mm (mots entiers, mots trop longs coupés).
fn wrap(font: &Font, text: &str, size: f32, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if font.width_mm(&candidate, size) <= max {
            cur = candidate;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        cur = String::new();
        for ch in word.chars() {
            let mut next = cur.clone();
            next.push(ch);
            if font.width_mm(&next, size) > max && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            cur.push(ch);
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

struct Pdf {
    doc: printpdf::PdfDocumentReference,
    layer: printpdf::PdfLayerReference,
    y: f32,
}

const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN: f32 = 20.0;

impl Pdf {
    fn new_page(&mut self) {
        let (p, l) = self.doc.add_page(printpdf::Mm(PAGE_W), printpdf::Mm(PAGE_H), "Calque");
        self.layer = self.doc.get_page(p).get_layer(l);
        self.y = PAGE_H - MARGIN;
    }

    /// Écrit une ligne de `size` pt ; saute de page si besoin.
    fn line(&mut self, text: &str, font: &Font, size: f32, x: f32) {
        let height = size * 0.3528 * 1.45;
        if self.y - height < MARGIN {
            self.new_page();
        }
        self.y -= height;
        if !text.is_empty() {
            self.layer.use_text(text, size, printpdf::Mm(x), printpdf::Mm(self.y), &font.reference);
        }
    }

    fn gap(&mut self, mm: f32) {
        self.y -= mm;
    }
}

/// PDF simple dessiné à la main (secours quand le moteur de rendu de l'application n'est pas disponible, et dans les tests).
pub fn write_pdf_basic(path: &Path, title: &str, md: &str) -> Result<(), String> {
    let (doc, p1, l1) = printpdf::PdfDocument::new(title, printpdf::Mm(PAGE_W), printpdf::Mm(PAGE_H), "Calque");
    let load = |names: &[&str]| -> Result<Font, String> {
        let bytes = font_file(names)?;
        let reference = doc.add_external_font(Cursor::new(bytes.clone())).map_err(|e| format!("Police illisible : {e}"))?;
        Ok(Font { reference, bytes, advances: RefCell::new(HashMap::new()) })
    };
    let regular = load(&["segoeui.ttf", "arial.ttf"])?;
    let bold = load(&["segoeuib.ttf", "arialbd.ttf"])?;
    let mono = load(&["consola.ttf", "cour.ttf"])?;
    let layer = doc.get_page(p1).get_layer(l1);
    let mut pdf = Pdf { doc, layer, y: PAGE_H - MARGIN };
    let text_w = PAGE_W - 2.0 * MARGIN;

    for l in wrap(&bold, title, 20.0, text_w) {
        pdf.line(&l, &bold, 20.0, MARGIN);
    }
    pdf.gap(5.0);

    for block in parse(md) {
        match block {
            Block::Heading(level, t) => {
                let size = [0.0, 16.0, 13.5, 12.0][level.clamp(1, 3) as usize];
                pdf.gap(3.0);
                for l in wrap(&bold, &t, size, text_w) {
                    pdf.line(&l, &bold, size, MARGIN);
                }
                pdf.gap(1.0);
            }
            Block::Para(t) => {
                for l in wrap(&regular, &t, 10.5, text_w) {
                    pdf.line(&l, &regular, 10.5, MARGIN);
                }
                pdf.gap(2.0);
            }
            Block::Item(marker, t) => {
                let indent = 7.0;
                for (i, l) in wrap(&regular, &t, 10.5, text_w - indent).iter().enumerate() {
                    if i == 0 {
                        pdf.line(l, &regular, 10.5, MARGIN + indent);
                        pdf.layer.use_text(marker.as_str(), 10.5, printpdf::Mm(MARGIN + 1.5), printpdf::Mm(pdf.y), &regular.reference);
                    } else {
                        pdf.line(l, &regular, 10.5, MARGIN + indent);
                    }
                }
                pdf.gap(0.8);
            }
            Block::Pre(lines) => {
                pdf.gap(1.0);
                for raw in lines {
                    for l in wrap_chars(&mono, &raw, 9.0, text_w - 4.0) {
                        pdf.line(&l, &mono, 9.0, MARGIN + 4.0);
                    }
                }
                pdf.gap(3.0);
            }
            Block::Rule => pdf.gap(4.0),
        }
    }

    let file = std::fs::File::create(path).map_err(|e| format!("Écriture impossible : {e}"))?;
    pdf.doc.save(&mut BufWriter::new(file)).map_err(|e| format!("PDF : {e}"))
}

/// Pour le code : coupe à la largeur sans toucher aux espaces.
fn wrap_chars(font: &Font, text: &str, size: f32, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        let mut next = cur.clone();
        next.push(ch);
        if font.width_mm(&next, size) > max && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    lines.push(cur);
    lines
}

// ---------------------------------------------------------------- Word

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn run(text: &str, props: &str) -> String {
    format!("<w:r><w:rPr>{props}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r>", xml(text))
}

fn paragraph(style: Option<&str>, indent: u32, runs: &str) -> String {
    let ppr = format!(
        "<w:pPr>{}{}</w:pPr>",
        style.map(|s| format!("<w:pStyle w:val=\"{s}\"/>")).unwrap_or_default(),
        if indent > 0 { format!("<w:ind w:left=\"{indent}\"/>") } else { String::new() }
    );
    format!("<w:p>{ppr}{runs}</w:p>")
}

pub fn write_docx(path: &Path, title: &str, md: &str) -> Result<(), String> {
    let mono = "<w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\"/><w:sz w:val=\"19\"/>";
    let mut body = paragraph(Some("Title"), 0, &run(title, ""));
    for block in parse(md) {
        match block {
            Block::Heading(level, t) => body.push_str(&paragraph(Some(&format!("Heading{level}")), 0, &run(&t, ""))),
            Block::Para(t) => body.push_str(&paragraph(None, 0, &run(&t, ""))),
            Block::Item(m, t) => body.push_str(&paragraph(None, 360, &run(&format!("{m} {t}"), ""))),
            Block::Pre(lines) => {
                for l in lines {
                    body.push_str(&paragraph(None, 360, &run(&l, mono)));
                }
            }
            Block::Rule => body.push_str(&paragraph(None, 0, "")),
        }
    }
    let ns = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let document = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document {ns}><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/></w:sectPr></w:body></w:document>");
    let style = |id: &str, name: &str, size: u32, bold: bool| {
        format!("<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:before=\"200\" w:after=\"80\"/></w:pPr><w:rPr>{}<w:sz w:val=\"{size}\"/></w:rPr></w:style>", if bold { "<w:b/>" } else { "" })
    };
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles {ns}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>{}{}{}{}</w:styles>",
        style("Title", "Title", 40, true),
        style("Heading1", "heading 1", 32, true),
        style("Heading2", "heading 2", 27, true),
        style("Heading3", "heading 3", 24, true)
    );
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
    let doc_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>";

    let file = std::fs::File::create(path).map_err(|e| format!("Écriture impossible : {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in [
        ("[Content_Types].xml", content_types.to_string()),
        ("_rels/.rels", rels.to_string()),
        ("word/_rels/document.xml.rels", doc_rels.to_string()),
        ("word/styles.xml", styles),
        ("word/document.xml", document),
    ] {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------- HTML

// ---------------------------------------------------------------- tableurs

/// Tableaux Markdown du texte (hors blocs de code) : lignes de cellules, ligne de séparation retirée.
pub fn tables(md: &str) -> Vec<Vec<Vec<String>>> {
    let mut out: Vec<Vec<Vec<String>>> = Vec::new();
    let mut cur: Vec<Vec<String>> = Vec::new();
    let mut in_code = false;
    let is_sep = |cells: &[String]| !cells.is_empty() && cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_code = !in_code;
        }
        if !in_code && t.starts_with('|') {
            let inner = t.trim_start_matches('|').trim_end_matches('|');
            let cells: Vec<String> = inner.split('|').map(|c| plain(c.trim())).collect();
            if !is_sep(&cells) {
                cur.push(cells);
            }
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// CSV pour Excel en français : séparateur « ; », UTF-8 avec BOM, champs entre guillemets au besoin.
pub fn to_csv(tables: &[Vec<Vec<String>>]) -> String {
    let field = |c: &str| {
        if c.contains([';', '"', '\n', '\r']) {
            format!("\"{}\"", c.replace('"', "\"\""))
        } else {
            c.to_string()
        }
    };
    let mut out = String::from("\u{feff}");
    for (i, t) in tables.iter().enumerate() {
        if i > 0 {
            out.push_str("\r\n");
        }
        for row in t {
            out.push_str(&row.iter().map(|c| field(c)).collect::<Vec<_>>().join(";"));
            out.push_str("\r\n");
        }
    }
    out
}

fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

/// Un nombre « propre » (sans zéro de tête, sans espace) devient une cellule numérique ; le reste reste du texte.
fn numeric(c: &str) -> Option<f64> {
    let t = c.trim();
    if t.is_empty() || (t.len() > 1 && t.starts_with('0') && !t.starts_with("0.") && !t.starts_with("0,")) || t.contains(' ') {
        return None;
    }
    t.replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite())
}

pub fn write_xlsx(path: &Path, tables: &[Vec<Vec<String>>]) -> Result<(), String> {
    let ns = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"";
    let mut sheets = Vec::new();
    for t in tables {
        let ncols = t.iter().map(Vec::len).max().unwrap_or(1);
        let mut widths = vec![8usize; ncols];
        let mut rows = String::new();
        for (r, row) in t.iter().enumerate() {
            rows.push_str(&format!("<row r=\"{}\">", r + 1));
            for (c, cell) in row.iter().enumerate() {
                widths[c] = widths[c].max(cell.chars().count().min(60) + 2);
                let at = format!("{}{}", col_name(c), r + 1);
                let style = if r == 0 { " s=\"1\"" } else { "" };
                match numeric(cell).filter(|_| r > 0) {
                    Some(n) => rows.push_str(&format!("<c r=\"{at}\"{style}><v>{n}</v></c>")),
                    None => rows.push_str(&format!("<c r=\"{at}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>", xml(cell))),
                }
            }
            rows.push_str("</row>");
        }
        let cols: String = widths.iter().enumerate().map(|(i, w)| format!("<col min=\"{0}\" max=\"{0}\" width=\"{w}\" customWidth=\"1\"/>", i + 1)).collect();
        sheets.push(format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet {ns}><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><cols>{cols}</cols><sheetData>{rows}</sheetData></worksheet>"));
    }
    let n = sheets.len();
    let overrides: String = (1..=n).map(|i| format!("<Override PartName=\"/xl/worksheets/sheet{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>")).collect();
    let content_types = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>{overrides}</Types>");
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let sheet_tags: String = (1..=n).map(|i| format!("<sheet name=\"{}\" sheetId=\"{i}\" r:id=\"rId{i}\"/>", if n == 1 { "Tableau".to_string() } else { format!("Tableau {i}") })).collect();
    let workbook = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook {ns} xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>{sheet_tags}</sheets></workbook>");
    let wb_rels: String = (1..=n).map(|i| format!("<Relationship Id=\"rId{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{i}.xml\"/>")).collect();
    let wb_rels = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{wb_rels}<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>", n + 1);
    // Deux styles : normal, et en-tête (gras sur fond gris clair).
    let styles = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><styleSheet {ns}><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill><fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFEDEDED\"/></patternFill></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"0\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\"/></cellXfs></styleSheet>");

    let file = std::fs::File::create(path).map_err(|e| format!("Écriture impossible : {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut parts: Vec<(String, String)> = vec![
        ("[Content_Types].xml".into(), content_types),
        ("_rels/.rels".into(), rels.to_string()),
        ("xl/workbook.xml".into(), workbook),
        ("xl/_rels/workbook.xml.rels".into(), wb_rels),
        ("xl/styles.xml".into(), styles),
    ];
    for (i, s) in sheets.into_iter().enumerate() {
        parts.push((format!("xl/worksheets/sheet{}.xml", i + 1), s));
    }
    for (name, data) in parts {
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Tableaux du texte, ou à défaut un texte déjà au format CSV (lignes de champs séparés par « ; » ou « , »).
fn tables_or_csv(md: &str) -> Result<Vec<Vec<Vec<String>>>, String> {
    let t = tables(md);
    if !t.is_empty() {
        return Ok(t);
    }
    let lines: Vec<&str> = md.lines().filter(|l| !l.trim().is_empty()).collect();
    for sep in [';', ','] {
        if lines.len() >= 2 && lines.iter().all(|l| l.contains(sep)) {
            return Ok(vec![lines.iter().map(|l| l.split(sep).map(|c| c.trim().trim_matches('"').to_string()).collect()).collect()]);
        }
    }
    Err("Aucun tableau dans ce texte : demande-le sous forme de tableau (colonnes séparées par « | »).".into())
}

// ---------------------------------------------------------------- point d'entrée

/// Fenêtre de l'application : donne accès au moteur de rendu pour imprimer les PDF.
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

pub fn set_app(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// PDF d'une page HTML : moteur de rendu de l'application, ou PDF simple si Windows ne peut pas l'imprimer.
pub fn html_to_pdf(path: &Path, title: &str, html: &str) -> Result<(), String> {
    match APP.get() {
        Some(app) => crate::webprint::html_to_pdf(app, html, path, title, "Lama Desktop"),
        None => Err("Moteur de rendu indisponible.".into()),
    }
}

/// PDF structuré : page de garde, sommaire, sections, tableaux, code, en-tête et numéros de page.
pub fn write_pdf(path: &Path, title: &str, md: &str, opts: &crate::docs::DocOpts) -> Result<(), String> {
    let html = crate::docs::document_html(title, md, opts);
    // Le titre en en-tête de page : celui du document (premier titre) s'il n'est pas donné.
    let header = if title.trim().is_empty() { md.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("Document").trim().to_string() } else { title.trim().to_string() };
    match html_to_pdf(path, &header, &html) {
        Ok(()) => Ok(()),
        Err(_) => write_pdf_basic(path, if title.trim().is_empty() { &header } else { title }, md),
    }
}

/// Écrit le document dans le format de l'extension du fichier : pdf, docx, xlsx, csv, html, md/txt.
pub fn write(path: &Path, title: &str, md: &str) -> Result<(), String> {
    write_with(path, title, md, &crate::docs::DocOpts::default())
}

pub fn write_with(path: &Path, title: &str, md: &str, opts: &crate::docs::DocOpts) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Dossier impossible : {e}"))?;
    }
    match ext.as_str() {
        "csv" => std::fs::write(path, to_csv(&tables_or_csv(md)?)).map_err(|er| format!("Écriture impossible : {er}")),
        "xlsx" => write_xlsx(path, &tables_or_csv(md)?),
        "pdf" => write_pdf(path, title, md, opts),
        "docx" => write_docx(path, title, md),
        "html" | "htm" => std::fs::write(path, crate::docs::document_html(title, md, opts)).map_err(|e| format!("Écriture impossible : {e}")),
        "md" | "txt" => std::fs::write(path, format!("# {title}\n\n{md}\n")).map_err(|e| format!("Écriture impossible : {e}")),
        other => Err(format!("Format inconnu : .{other} (pdf, docx, xlsx, csv, html, md, txt)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "## Plan\n\nUn paragraphe avec du **gras**, du `code` et un [lien](https://x.fr).\n\n- première puce\n- seconde puce\n1. un\n2. deux\n\n```rust\nfn main() {\n    println!(\"é\");\n}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";

    #[test]
    fn parses_blocks() {
        let b = parse(SAMPLE);
        assert_eq!(b[0], Block::Heading(2, "Plan".into()));
        assert_eq!(b[1], Block::Para("Un paragraphe avec du gras, du code et un lien.".into()));
        assert_eq!(b[2], Block::Item("•".into(), "première puce".into()));
        assert_eq!(b[4], Block::Item("1.".into(), "un".into()));
        assert!(matches!(&b[6], Block::Pre(l) if l.len() == 3));
        assert!(matches!(&b[7], Block::Pre(l) if l.len() == 3), "tableau groupé");
    }

    #[test]
    #[ignore = "génère un exemple à regarder"]
    fn sample_pdf() {
        let md = format!("{SAMPLE}
## Suite

{}", "Un long paragraphe de démonstration avec des accents : éàùç œuvre. ".repeat(40));
        write(&std::env::temp_dir().join("lama-sample.pdf"), "Exemple de rapport", &md).unwrap();
    }

    const CALENDAR: &str = "Voici le planning :\n\n| Date | Réseau | Texte | Vues |\n|---|:---:|---|--:|\n| 2026-10-20 | **LinkedIn** | Annonce; « nouveauté » | 1200 |\n| 2026-10-21 | Instagram | Photo du jour | 0042 |\n\n```\n| pas | un tableau |\n```\n";

    #[test]
    fn extracts_tables_and_skips_code() {
        let t = tables(CALENDAR);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].len(), 3, "séparateur retiré");
        assert_eq!(t[0][1], vec!["2026-10-20", "LinkedIn", "Annonce; « nouveauté »", "1200"]);
    }

    #[test]
    fn csv_is_excel_friendly() {
        let csv = to_csv(&tables(CALENDAR));
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Date;Réseau;Texte;Vues\r\n"));
        assert!(csv.contains("\"Annonce; « nouveauté »\""), "{csv}");
    }

    #[test]
    #[ignore = "génère un exemple à ouvrir dans Excel"]
    fn sample_xlsx() {
        write(&std::env::temp_dir().join("lama-sample.xlsx"), "Planning", CALENDAR).unwrap();
        write(&std::env::temp_dir().join("lama-sample.csv"), "Planning", CALENDAR).unwrap();
    }

    #[test]
    fn xlsx_has_header_style_numbers_and_text() {
        let d = std::env::temp_dir().join(format!("xlsx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("planning.xlsx");
        write(&p, "Planning", CALENDAR).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&p).unwrap()).unwrap();
        let mut sheet = String::new();
        std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
        assert!(sheet.contains("<c r=\"A1\" t=\"inlineStr\" s=\"1\">") && sheet.contains("Réseau"));
        assert!(sheet.contains("<c r=\"D2\"><v>1200</v></c>"), "nombre en cellule numérique : {sheet}");
        assert!(sheet.contains("<t xml:space=\"preserve\">0042</t>"), "zéro de tête conservé en texte");
        assert!(z.by_name("xl/styles.xml").is_ok() && z.by_name("xl/workbook.xml").is_ok());
        assert!(write(&d.join("x.xlsx"), "t", "Pas de tableau ici.").is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn writes_every_format() {
        let d = std::env::temp_dir().join(format!("export-{}", uuid::Uuid::new_v4()));
        let long = format!("{SAMPLE}\n{}", "Texte long avec accents éàù. ".repeat(900));
        for ext in ["pdf", "docx", "html", "md"] {
            let p = d.join(format!("doc.{ext}"));
            write(&p, "Mon « titre »", &long).unwrap_or_else(|e| panic!("{ext}: {e}"));
            assert!(std::fs::metadata(&p).unwrap().len() > 500, "{ext}");
        }
        assert!(std::fs::read(d.join("doc.pdf")).unwrap().starts_with(b"%PDF"));
        let back = crate::pdf::extract_text(&d.join("doc.pdf")).unwrap();
        assert!(back.contains("première puce") && back.contains("Plan"), "texte relu : {}", &back[..back.len().min(300)]);
        let mut z = zip::ZipArchive::new(std::fs::File::open(d.join("doc.docx")).unwrap()).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut xml).unwrap();
        assert!(xml.contains("première puce") && xml.contains("Mon « titre »"));
        assert!(std::fs::read_to_string(d.join("doc.html")).unwrap().contains("Plan</h2>"));
        assert!(write(&d.join("x.exe"), "t", "m").is_err());
        let _ = std::fs::remove_dir_all(d);
    }
}
