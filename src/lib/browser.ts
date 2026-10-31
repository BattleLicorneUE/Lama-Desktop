/** Adresses du navigateur intégré. */

export const SEARCH = "https://duckduckgo.com/?q=";

export const searchUrl = (query: string) => SEARCH + encodeURIComponent(query.trim());

/** Ce que l'utilisateur tape dans la barre : une adresse complète, un nom de site, ou une recherche. */
export function toAddress(input: string): string {
  const t = input.trim();
  if (!t) return SEARCH.replace("?q=", "");
  if (/^https?:\/\//i.test(t)) return t;
  if (!/\s/.test(t) && /^([a-z0-9-]+\.)+[a-z]{2,}(:\d+)?([/?#].*)?$/i.test(t)) return "https://" + t;
  return searchUrl(t);
}

/** Page que Lama consulte avec un outil web : sa recherche, ou la page lue. Null pour tout autre outil. */
export function urlForTool(name: string, args: Record<string, unknown>): string | null {
  if (name === "web_search") {
    const q = String(args.query ?? "").trim();
    return q ? searchUrl(q) : null;
  }
  if (name === "web_fetch") {
    const u = String(args.url ?? "").trim();
    return /^https?:\/\//i.test(u) ? u : null;
  }
  return null;
}

/** Adresse lisible pour la barre : « d%C3%A9grad%C3%A9 » devient « dégradé ». Ne sert qu'à l'affichage. */
export function pretty(url: string): string {
  try {
    return decodeURI(url);
  } catch {
    return url;
  }
}
