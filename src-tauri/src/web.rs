//! Recherche web et lecture de pages pour l'agent.

use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;


use serde_json::Value;

const UA: &str = concat!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) LamaDesktop/", env!("CARGO_PKG_VERSION"));
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
    // Les entités numériques (&#039;, &#x27;, &#233;…) d'abord ; « &amp; » en dernier pour ne pas décoder deux fois.
    let s = re!(r"&#(x[0-9a-fA-F]+|[0-9]+);").replace_all(s, |c: &regex::Captures| {
        let n = &c[1];
        let code = match n.strip_prefix('x') {
            Some(h) => u32::from_str_radix(h, 16).ok(),
            None => n.parse().ok(),
        };
        code.and_then(char::from_u32).map(String::from).unwrap_or_else(|| c[0].to_string())
    });
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&nbsp;", " ").replace("&amp;", "&")
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
                // 192.0.0.0/24 (protocoles IETF), 198.18.0.0/15 (tests), multicast et réservé (224.0.0.0 et au-dessus)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) || (o[0] == 198 && (o[1] & 0xfe) == 18) || o[0] >= 224
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4() {
                return is_private_ip(IpAddr::V4(v4));
            }
            // 64:ff9b::/96 (NAT64) enveloppe une IPv4 dans ses 32 derniers bits.
            let s = v.segments();
            if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
                return is_private_ip(IpAddr::V4(std::net::Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8)));
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

/// DuckDuckGo répond « 202 » ou une page de vérification quand il soupçonne un robot : sans résultat, ce n'est pas « aucun résultat ».
fn ddg_blocked(status: u16, html: &str) -> bool {
    let h = html.to_lowercase();
    status == 202 || h.contains("anomaly") || h.contains("challenge") || h.contains("captcha")
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

/// Internet répond-il ? On interroge l'adresse que Windows utilise lui-même pour savoir s'il est en ligne : elle ne dépend ni de
/// DuckDuckGo (qui peut être lent ou bloqué sans que l'ordinateur soit hors ligne) ni d'un site tiers de plus.
pub async fn online() -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build() else { return false };
    client.get("http://www.msftconnecttest.com/connecttest.txt").send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

/// Résultats de l'API officielle de Wikipédia (sans clé) : un secours étroit mais fiable quand DuckDuckGo ne répond pas.
pub fn parse_wikipedia(json: &Value, lang: &str) -> Vec<Hit> {
    json["query"]["search"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let title = r["title"].as_str()?.to_string();
            let mut url = reqwest::Url::parse(&format!("https://{lang}.wikipedia.org/wiki/")).ok()?;
            url.path_segments_mut().ok()?.pop_if_empty().push(&title.replace(' ', "_"));
            Some(Hit { title: format!("{title} — Wikipédia"), url: url.to_string(), snippet: html_to_text(r["snippet"].as_str().unwrap_or("")) })
        })
        .collect()
}

async fn wikipedia(query: &str) -> Result<Vec<Hit>, String> {
    // Wikimédia demande un User-Agent qui dit qui parle.
    let client = reqwest::Client::builder()
        .user_agent(concat!("LamaDesktop/", env!("CARGO_PKG_VERSION"), " (https://github.com/BattleLicorneUE/Lama-Desktop)"))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let mut hits = Vec::new();
    for lang in ["fr", "en"] {
        let resp = client
            .get(format!("https://{lang}.wikipedia.org/w/api.php"))
            .query(&[("action", "query"), ("list", "search"), ("srsearch", query), ("srlimit", "5"), ("format", "json"), ("utf8", "1")])
            .send()
            .await
            .map_err(|e| format!("Wikipédia injoignable : {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Wikipédia a répondu {}", resp.status()));
        }
        hits.extend(parse_wikipedia(&resp.json().await.map_err(|e| e.to_string())?, lang));
        if hits.len() >= 3 {
            break;
        }
    }
    Ok(hits)
}

/// Recherche web : DuckDuckGo (sans compte ni clé) ; s'il ne répond pas ou refuse, Wikipédia en secours, en le disant.
pub async fn search(query: &str) -> Result<String, String> {
    match duckduckgo(query).await {
        Ok(out) => Ok(out),
        Err(ddg_error) => match wikipedia(query).await {
            Ok(hits) if !hits.is_empty() => Ok(format!("(DuckDuckGo n'a pas répondu : {ddg_error} Voici seulement des résultats de Wikipédia, à prendre comme tels : une encyclopédie ne couvre pas l'actualité ni le détail technique.)
{}", format_hits(&hits[..hits.len().min(8)]))),
            _ => Err(ddg_error),
        },
    }
}

async fn duckduckgo(query: &str) -> Result<String, String> {
    let client = http()?;
    let resp = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "DuckDuckGo ne répond pas (délai de 10 s dépassé). Le site est peut-être bloqué ou surchargé : réessaie plus tard.".to_string() } else { format!("Recherche échouée : {e}") })?;
    if !resp.status().is_success() {
        return Err(format!("DuckDuckGo a répondu {}", resp.status()));
    }
    let status = resp.status();
    let html = resp.text().await.map_err(|e| e.to_string())?;
    let hits = parse_ddg(&html);
    if hits.is_empty() && ddg_blocked(status.as_u16(), &html) {
        return Err("DuckDuckGo a refusé la recherche (il prend l'application pour un robot). Réessaie plus tard, ou choisis un autre moteur dans Réglages.".into());
    }
    Ok(format_hits(&hits[..hits.len().min(8)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn html_to_text_strips_markup() {
        let t = html_to_text("<html><head><title>x</title></head><body><script>bad()</script><h1>Titre</h1><p>Un &amp; deux</p></body></html>");
        assert_eq!(t, "Titre\n\nUn & deux");
    }

    #[test]
    fn decodes_numeric_entities_once() {
        assert_eq!(decode_entities("l&#039;accent &#233;t&#xE9; &amp;lt;b&amp;gt;"), "l'accent été &lt;b&gt;");
        assert_eq!(decode_entities("&#99999999999;"), "&#99999999999;");
    }

    #[test]
    fn percent_decode_handles_utf8_and_edges() {
        assert_eq!(percent_decode("https%3A%2F%2Fa.fr%2Fcaf%C3%A9"), "https://a.fr/café");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a+b"), "a b");
    }

    #[test]
    fn recognises_a_duckduckgo_block_page() {
        assert!(ddg_blocked(202, ""));
        assert!(ddg_blocked(200, "<div class=\"anomaly-modal\">Unfortunately, bots use DuckDuckGo too.</div>"));
        assert!(!ddg_blocked(200, "<div class=\"no-results\">Aucun résultat</div>"));
    }

    #[test]
    fn parses_wikipedia_results() {
        let json: Value = serde_json::from_str(r#"{"query":{"search":[{"title":"Rust (langage)","snippet":"Pour les <span class=\"searchmatch\">Rust</span> et Mozilla"},{"title":"Café","snippet":""}]}}"#).unwrap();
        let hits = parse_wikipedia(&json, "fr");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://fr.wikipedia.org/wiki/Rust_(langage)");
        assert_eq!(hits[0].title, "Rust (langage) — Wikipédia");
        assert_eq!(hits[0].snippet, "Pour les Rust et Mozilla");
        assert_eq!(hits[1].url, "https://fr.wikipedia.org/wiki/Caf%C3%A9", "les accents sont encodés");
        assert!(parse_wikipedia(&json!({}), "fr").is_empty());
    }

    /// Test réseau : le secours Wikipédia (cargo test live_wikipedia -- --ignored --nocapture).
    /// Test réseau : la recherche complète, avec secours si DuckDuckGo ne répond pas.
    #[tokio::test]
    #[ignore]
    async fn live_search_with_fallback() {
        let out = search("langage de programmation rust").await.unwrap();
        println!("{out}");
        assert!(out.contains("https://"), "{out}");
    }

    #[tokio::test]
    #[ignore]
    async fn live_wikipedia_fallback() {
        let hits = wikipedia("langage de programmation rust").await.unwrap();
        for h in &hits {
            println!("{} | {} | {}", h.title, h.url, h.snippet);
        }
        assert!(!hits.is_empty() && hits[0].url.starts_with("https://fr.wikipedia.org/wiki/"));
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
        let out = search("rust programming language").await.unwrap();
        println!("{out}");
        assert!(out.contains("http"), "aucun résultat analysé");
        let page = fetch("https://example.com").await.unwrap();
        println!("--- PAGE ---\n{page}");
        assert!(page.contains("Example Domain"), "contenu inattendu : {page}");
    }

    /// Test réseau : la vérification de la commande `net_online` (cargo test live_internet -- --ignored).
    #[tokio::test]
    #[ignore]
    async fn live_internet_is_reachable() {
        let t = std::time::Instant::now();
        let ok = online().await;
        println!("joignable : {ok} en {:?}", t.elapsed());
        assert!(ok);
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
        // Plages réservées, multicast et NAT64 qui enveloppe une adresse locale.
        for bad in ["192.0.0.8", "198.18.0.1", "198.19.255.255", "224.0.0.1", "255.255.255.255", "64:ff9b::7f00:1", "64:ff9b::a00:1"] {
            assert!(is_private_ip(bad.parse().unwrap()), "{bad}");
        }
        assert!(!is_private_ip("64:ff9b::808:808".parse().unwrap()), "NAT64 vers une adresse publique");
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
