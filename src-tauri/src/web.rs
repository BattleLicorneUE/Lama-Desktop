//! Recherche web et lecture de pages pour l'agent.

use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Value};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) LamaDesktop/0.1";
const MAX_PAGE_BYTES: usize = 2_000_000;
const MAX_TEXT: usize = 15_000;

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())
}

macro_rules! re {
    ($pat:expr) => {{
        static R: OnceLock<Regex> = OnceLock::new();
        R.get_or_init(|| Regex::new($pat).unwrap())
    }};
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
        .replace("&#x27;", "'").replace("&#39;", "'").replace("&nbsp;", " ")
}

/// HTML -> texte lisible (sans scripts, styles ni balises).
pub fn html_to_text(html: &str) -> String {
    let s = re!(r"(?is)<(script|style|noscript|svg|head)\b.*?</(script|style|noscript|svg|head)>").replace_all(html, " ");
    let s = re!(r"(?i)</?(p|div|br|li|tr|h[1-6]|section|article)\b[^>]*>").replace_all(&s, "\n");
    let s = re!(r"(?s)<[^>]*>").replace_all(&s, " ");
    let s = decode_entities(&s);
    let s = re!(r"[ \t\r\f]+").replace_all(&s, " ");
    re!(r"\n\s*\n+").replace_all(s.trim(), "\n\n").into_owned()
}

/// Adresse du réseau local, d'un réseau privé ou réservée : jamais atteignable depuis une page web.
/// Les adresses IPv6 qui enveloppent une IPv4 (`::ffff:127.0.0.1`) sont jugées comme cette IPv4.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_private() || v.is_loopback() || v.is_link_local() || v.is_unspecified() || v.is_broadcast()
                // 100.64.0.0/10 (réseau opérateur) et 0.0.0.0/8
                || (o[0] == 100 && (o[1] & 0xc0) == 64) || o[0] == 0
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4() {
                return is_private_ip(IpAddr::V4(v4));
            }
            v.is_loopback() || v.is_unspecified() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Refuse les adresses locales ou privées (évite d'atteindre le réseau interne).
pub fn check_url(url: &str) -> Result<reqwest::Url, String> {
    let u = reqwest::Url::parse(url).map_err(|_| format!("URL invalide : {url}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("Seules les URL http(s) sont autorisées".into());
    }
    // Le point final d'un nom (« localhost. ») désigne le même hôte.
    let host = u.host_str().ok_or("URL sans hôte")?.to_lowercase();
    let host = host.trim_end_matches('.');
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") || host.ends_with(".internal") {
        return Err("Adresse locale refusée".into());
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err("Adresse privée refusée".into());
        }
    }
    Ok(u)
}

/// Client pour une seule requête : le nom de l'hôte est résolu ici, refusé s'il pointe vers une adresse privée, puis figé
/// (la requête ne refait pas de résolution DNS, qu'un serveur pourrait détourner entre-temps). Les redirections sont suivies à la main.
async fn pinned_client(u: &reqwest::Url) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder().user_agent(UA).timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none());
    let host = u.host_str().ok_or("URL sans hôte")?.trim_matches(['[', ']']).to_string();
    let host = host.as_str();
    if host.parse::<IpAddr>().is_err() {
        let port = u.port_or_known_default().unwrap_or(443);
        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port)).await.map_err(|e| format!("Résolution de {host} impossible : {e}"))?.collect();
        if addrs.is_empty() {
            return Err(format!("Hôte introuvable : {host}"));
        }
        if addrs.iter().any(|a| is_private_ip(a.ip())) {
            return Err(format!("Adresse privée refusée : {host} pointe vers le réseau local"));
        }
        b = b.resolve_to_addrs(host, &addrs);
    }
    b.build().map_err(|e| e.to_string())
}

/// GET qui revérifie chaque étape : l'adresse de départ comme celles des redirections (5 au plus).
async fn get_checked(url: &str) -> Result<reqwest::Response, String> {
    let mut u = check_url(url)?;
    for _ in 0..=5 {
        let resp = pinned_client(&u).await?.get(u.clone()).send().await.map_err(|e| format!("Requête échouée : {e}"))?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        let next = resp.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).ok_or("Redirection sans destination")?;
        u = check_url(u.join(next).map_err(|_| "Redirection invalide")?.as_str()).map_err(|e| format!("Redirection refusée : {e}"))?;
    }
    Err("Trop de redirections".into())
}

pub async fn fetch(url: &str) -> Result<String, String> {
    let resp = get_checked(url).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("La page a répondu {status}"));
    }
    let ctype = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
    if !(ctype.is_empty() || ctype.contains("text") || ctype.contains("json") || ctype.contains("xml")) {
        return Err(format!("Type de contenu non pris en charge : {ctype}"));
    }
    let mut bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    bytes.truncate(MAX_PAGE_BYTES);
    let body = String::from_utf8_lossy(&bytes).into_owned();
    let mut text = if ctype.contains("html") || body.trim_start().starts_with('<') {
        let title = re!(r"(?is)<title[^>]*>(.*?)</title>")
            .captures(&body)
            .map(|c| decode_entities(c[1].trim()))
            .filter(|t| !t.is_empty());
        let text = html_to_text(&body);
        match title {
            Some(t) => format!("Titre : {t}\n\n{text}"),
            None => text,
        }
    } else {
        body
    };
    if text.len() > MAX_TEXT {
        let cut = (0..=MAX_TEXT).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("\n[… tronqué …]");
    }
    Ok(text)
}

#[derive(Debug, PartialEq)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Extrait les résultats de la page HTML de DuckDuckGo.
pub fn parse_ddg(html: &str) -> Vec<Hit> {
    let link = re!(r#"(?is)<a[^>]*class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#);
    let snippet = re!(r#"(?is)<a[^>]*class="result__snippet"[^>]*>(.*?)</a>"#);
    let snippets: Vec<String> = snippet.captures_iter(html).map(|c| html_to_text(&c[1])).collect();
    link.captures_iter(html)
        .enumerate()
        .map(|(i, c)| {
            let mut url = decode_entities(&c[1]);
            if let Some(pos) = url.find("uddg=") {
                let enc = url[pos + 5..].split('&').next().unwrap_or("");
                url = percent_decode(enc);
            } else if url.starts_with("//") {
                url = format!("https:{url}");
            }
            Hit { title: html_to_text(&c[2]), url, snippet: snippets.get(i).cloned().unwrap_or_default() }
        })
        .filter(|h| h.url.starts_with("http"))
        .collect()
}

fn format_hits(hits: &[Hit]) -> String {
    if hits.is_empty() {
        return "Aucun résultat.".into();
    }
    hits.iter()
        .enumerate()
        .map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recherche via l'API web d'Ollama si une clé est fournie, sinon via DuckDuckGo.
pub async fn search(query: &str, ollama_key: Option<&str>) -> Result<String, String> {
    let client = http()?;
    if let Some(key) = ollama_key.filter(|k| !k.trim().is_empty()) {
        let resp = client
            .post("https://ollama.com/api/web_search")
            .bearer_auth(key.trim())
            .json(&json!({ "query": query, "max_results": 6 }))
            .send()
            .await
            .map_err(|e| format!("Recherche échouée : {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("L'API de recherche Ollama a répondu {}", resp.status()));
        }
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        let hits: Vec<Hit> = v["results"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| Hit {
                        title: r["title"].as_str().unwrap_or("").into(),
                        url: r["url"].as_str().unwrap_or("").into(),
                        snippet: r["content"].as_str().unwrap_or("").chars().take(400).collect(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Ok(format_hits(&hits));
    }
    let resp = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .await
        .map_err(|e| format!("Recherche échouée : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("DuckDuckGo a répondu {}", resp.status()));
    }
    let hits = parse_ddg(&resp.text().await.map_err(|e| e.to_string())?);
    Ok(format_hits(&hits[..hits.len().min(8)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_strips_markup() {
        let t = html_to_text("<html><head><title>x</title></head><body><script>bad()</script><h1>Titre</h1><p>Un &amp; deux</p></body></html>");
        assert_eq!(t, "Titre\n\nUn & deux");
    }

    #[test]
    fn percent_decode_handles_utf8_and_edges() {
        assert_eq!(percent_decode("https%3A%2F%2Fa.fr%2Fcaf%C3%A9"), "https://a.fr/café");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a+b"), "a b");
    }

    #[test]
    fn parses_duckduckgo_results() {
        let html = r#"<a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fp&amp;rut=1">Exemple <b>un</b></a>
            <a class="result__snippet" href="x">Un <b>extrait</b></a>"#;
        let hits = parse_ddg(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/p");
        assert_eq!(hits[0].title, "Exemple un");
        assert_eq!(hits[0].snippet, "Un extrait");
    }

    /// Test réseau : `cargo test live_ -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn live_search_and_fetch() {
        let out = search("rust programming language", None).await.unwrap();
        println!("{out}");
        assert!(out.contains("http"), "aucun résultat analysé");
        let page = fetch("https://example.com").await.unwrap();
        println!("--- PAGE ---\n{page}");
        assert!(page.contains("Example Domain"), "contenu inattendu : {page}");
    }

    #[test]
    fn blocks_local_and_private_urls() {
        assert!(check_url("https://example.com/a").is_ok());
        assert!(check_url("http://localhost:11434/api").is_err());
        assert!(check_url("http://127.0.0.1/").is_err());
        assert!(check_url("http://192.168.1.10/").is_err());
        assert!(check_url("http://[::1]/").is_err());
        assert!(check_url("file:///C:/x").is_err());
    }

    #[test]
    fn disguised_local_addresses_are_blocked() {
        for u in [
            "http://localhost./", "http://LOCALHOST/", "http://[::ffff:127.0.0.1]/", "http://[::ffff:192.168.0.1]/", "http://[fe80::1]/",
            "http://2130706433/", "http://0x7f.0.0.1/", "http://127.1/", "http://100.64.0.1/", "http://169.254.169.254/", "http://0.0.0.0/", "http://[fd00::1]/",
        ] {
            assert!(check_url(u).is_err(), "{u} devrait être refusée");
        }
        assert!(check_url("https://93.184.216.34/").is_ok());
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()) && !is_private_ip("2606:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn a_name_that_resolves_to_the_local_network_is_refused() {
        // « localhost » se résout par le fichier hosts, sans réseau : le contrôle après résolution doit le refuser.
        let u = reqwest::Url::parse("http://localhost:8080/").unwrap();
        let err = pinned_client(&u).await.unwrap_err();
        assert!(err.contains("privée"), "{err}");
        assert!(get_checked("http://localhost/").await.is_err());
    }

    #[tokio::test]
    async fn redirects_to_local_addresses_are_refused() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf).await;
                let _ = s.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
        });
        // Le serveur de test est lui-même local : on exerce donc la boucle de redirection sur sa réponse en la rejouant à la main.
        let raw = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap().get(format!("http://127.0.0.1:{port}/")).send().await.unwrap();
        let loc = raw.headers().get(reqwest::header::LOCATION).unwrap().to_str().unwrap().to_string();
        assert!(raw.status().is_redirection());
        assert!(check_url(&loc).is_err(), "la destination de la redirection doit être refusée");
    }
}
