/** Artefacts : contenus générés par le modèle qu'on affiche à côté de la conversation (page web, SVG, composant React, diagramme, document). */

export type Artifact = { code: string; lang: string; title: string };

const ALIASES: Record<string, string> = { jsx: "react", tsx: "react", md: "markdown", mmd: "mermaid" };

/** Langages de blocs de code qui ont un aperçu. */
export const ARTIFACT_LANGS = ["html", "svg", "react", "jsx", "tsx", "mermaid", "mmd", "markdown", "md"];

export const normLang = (l: string) => ALIASES[l.toLowerCase()] ?? l.toLowerCase();

export const LANG_LABEL: Record<string, string> = { html: "Page web", svg: "Image SVG", react: "Composant React", mermaid: "Diagramme", markdown: "Document" };

/** Ce bloc mérite-t-il un aperçu ? (les petits extraits de code ne sont pas des artefacts) */
export function isArtifact(lang: string, code: string): boolean {
  const l = normLang(lang);
  if (!ARTIFACT_LANGS.includes(lang.toLowerCase())) return false;
  if (l === "svg") return code.includes("<svg");
  if (l === "html") return code.length > 40;
  if (l === "react") return /export\s+default|function\s+[A-Z]\w*\s*\(|const\s+[A-Z]\w*\s*=/.test(code) && code.length > 80;
  if (l === "mermaid") return /^\s*(graph|flowchart|sequenceDiagram|classDiagram|stateDiagram|erDiagram|gantt|pie|journey|mindmap|timeline|gitGraph|quadrantChart)/m.test(code);
  if (l === "markdown") return code.length > 300 && /^#{1,3}\s/m.test(code);
  return false;
}

function titleOf(lang: string, code: string): string {
  const l = normLang(lang);
  const t = /<title>([^<]{1,60})<\/title>/i.exec(code)?.[1] ?? /^#\s+(.{1,60})$/m.exec(code)?.[1] ?? /export\s+default\s+function\s+(\w+)/.exec(code)?.[1];
  return (t ?? LANG_LABEL[l] ?? l).trim();
}

/** Tous les artefacts de la conversation, dans l'ordre : ce sont les « versions » entre lesquelles on navigue. */
export function collectArtifacts(messages: { role: string; content: string }[]): Artifact[] {
  const out: Artifact[] = [];
  for (const m of messages) {
    if (m.role !== "assistant") continue;
    for (const b of m.content.matchAll(/```([A-Za-z]+)[^\n]*\n([\s\S]*?)```/g)) {
      if (isArtifact(b[1], b[2])) out.push({ code: b[2].trim(), lang: normLang(b[1]), title: titleOf(b[1], b[2]) });
    }
  }
  return out;
}

/** Dernier artefact « complet » d'une réponse : une page HTML entière, un SVG, un composant, un diagramme ou un document. */
export function lastOpenable(text: string): Artifact | null {
  const found = collectArtifacts([{ role: "assistant", content: text }]).filter((a) =>
    a.lang === "html" ? /<!doctype html|<html[\s>]/i.test(a.code) && a.code.length > 200 : a.lang === "svg" ? a.code.length > 200 : true,
  );
  return found.at(-1) ?? null;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// Document isolé : scripts autorisés mais sans accès à l'application (pas de allow-same-origin) ni aux données ;
// seules quelques bibliothèques courantes peuvent être chargées depuis un CDN.
const CSP =
  "default-src 'none'; img-src data: blob: https:; media-src data: blob:; connect-src https://cdnjs.cloudflare.com https://cdn.jsdelivr.net; style-src 'unsafe-inline' https://fonts.googleapis.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net; font-src data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net; script-src 'unsafe-inline' 'unsafe-eval' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://cdn.tailwindcss.com";
const GUARD = `<meta http-equiv="Content-Security-Policy" content="${CSP}">`;

const ERROR_BOX =
  "window.addEventListener('error',function(e){var r=document.getElementById('root')||document.body;r.innerHTML='<pre style=\"color:#b3261e;white-space:pre-wrap;font:13px/1.5 ui-monospace,monospace;padding:16px\">'+String(e.message).replace(/</g,'&lt;')+'</pre>'});";

/** Prépare le code d'un composant React : retire les imports, repère le composant à afficher. */
export function reactSource(code: string): { body: string; name: string } {
  let name = /export\s+default\s+(?:function|class)\s+([A-Za-z_]\w*)/.exec(code)?.[1] ?? /export\s+default\s+([A-Za-z_]\w*)\s*;?\s*$/m.exec(code)?.[1];
  const body = code
    .replace(/^\s*import\s[^;]*?from\s+['"][^'"]+['"];?\s*$/gm, "")
    .replace(/^\s*import\s+['"][^'"]+['"];?\s*$/gm, "")
    .replace(/export\s+default\s+(function|class)\s+/g, "$1 ")
    .replace(/^\s*export\s+default\s+[A-Za-z_]\w*\s*;?\s*$/gm, "")
    .replace(/^\s*export\s+(?=(?:const|function|class|let|var)\b)/gm, "");
  name ??= /(?:function|class)\s+(App)\b|const\s+(App)\s*=/.exec(body)?.[0] ? "App" : /(?:function\s+|const\s+)([A-Z]\w*)/.exec(body)?.[1] ?? "App";
  return { body, name };
}

/** Document HTML complet à afficher dans le cadre d'aperçu. */
export function buildDoc(lang: string, code: string, dark: boolean): string {
  const l = normLang(lang);
  const fg = dark ? "#e8e6e1" : "#1f1f1d";
  const bg = dark ? "#1f1f1e" : "#ffffff";
  if (l === "svg") {
    return `<!doctype html><html><head>${GUARD}</head><body style="margin:0;display:grid;place-items:center;min-height:100vh;background:${bg}">${code}</body></html>`;
  }
  if (l === "mermaid") {
    return `<!doctype html><html><head>${GUARD}<style>body{margin:0;padding:16px;display:grid;place-items:center;min-height:100vh;background:${bg};color:${fg};font-family:system-ui}</style></head><body><pre class="mermaid">${esc(code)}</pre>
<script src="https://cdn.jsdelivr.net/npm/mermaid@10.9.1/dist/mermaid.min.js"></script>
<script>${ERROR_BOX}mermaid.initialize({startOnLoad:false,theme:${dark ? "'dark'" : "'default'"},securityLevel:'strict'});mermaid.run().catch(function(e){document.body.innerHTML='<pre style="color:#b3261e;white-space:pre-wrap">'+String(e.message||e).replace(/</g,'&lt;')+'</pre>'});</script></body></html>`;
  }
  if (l === "react") {
    const { body, name } = reactSource(code);
    // Les « </script> » du code ne doivent pas fermer la balise.
    const safe = body.replace(/<\/script/gi, "<\\/script");
    return `<!doctype html><html><head>${GUARD}<style>body{margin:0;font-family:system-ui,sans-serif}</style>
<script src="https://cdn.tailwindcss.com"></script></head><body><div id="root"></div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/react/18.3.1/umd/react.production.min.js"></script>
<script src="https://cdnjs.cloudflare.com/ajax/libs/react-dom/18.3.1/umd/react-dom.production.min.js"></script>
<script src="https://cdnjs.cloudflare.com/ajax/libs/babel-standalone/7.24.7/babel.min.js"></script>
<script>${ERROR_BOX}</script>
<script type="text/babel" data-presets="react">
const {useState,useEffect,useRef,useMemo,useCallback,useReducer,useContext,useLayoutEffect,createContext,Fragment,memo,forwardRef} = React;
${safe}
ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(${name}));
</script></body></html>`;
  }
  // html : page complète ou fragment
  return /<head[\s>]/i.test(code)
    ? code.replace(/<head([^>]*)>/i, `<head$1>${GUARD}`)
    : `<!doctype html><html><head>${GUARD}</head><body>${code}</body></html>`;
}

export const EXT: Record<string, string> = { html: "html", svg: "svg", react: "jsx", mermaid: "mmd", markdown: "md" };

/** Types de création proposés dans la page Artéfacts : l'invite de départ et la consigne donnée au modèle. */
export type CreationKind = "document" | "slides" | "design" | "dashboard";

export const CREATIONS: Record<CreationKind, { label: string; prompt: string; hint: string }> = {
  document: {
    label: "Documents",
    prompt: "Rédige un document sur : ",
    hint: "Mode Documents : produis le document demandé en UN SEUL bloc ```markdown complet : titre « # », résumé en citation « > », sections « ## », listes et tableaux quand ils aident, conclusion ou prochaines étapes. Une phrase au plus avant le bloc, rien après.",
  },
  slides: {
    label: "Diapositives",
    prompt: "Crée une présentation sur : ",
    hint: "Mode Diapositives : produis un diaporama en UN SEUL fichier HTML complet (```html) : une section par diapositive au format 16:9, navigation aux flèches du clavier et par boutons, numéro de diapositive, style sobre et soigné, aucune bibliothèque externe, texte concis (6 lignes au plus par diapositive). Une phrase au plus avant le bloc.",
  },
  design: {
    label: "Design",
    prompt: "Conçois une maquette de : ",
    hint: "Mode Design : produis une maquette en UN SEUL fichier HTML complet (```html), responsive et soignée (typographie, espacements, palette cohérente, survol des boutons), sans bibliothèque externe, avec un contenu réaliste. Une phrase au plus avant le bloc.",
  },
  dashboard: {
    label: "Tableau de bord",
    prompt: "Construis un tableau de bord pour : ",
    hint: "Mode Tableau de bord : produis un tableau de bord en UN SEUL fichier HTML complet (```html) : chiffres clés, graphiques dessinés en SVG ou CSS (sans bibliothèque), données d'exemple réalistes écrites dans la page, grille responsive. Une phrase au plus avant le bloc.",
  },
};

/** Type d'un artefact enregistré : celui choisi à la création, sinon déduit du langage. */
export function kindOf(kind: string, lang: string): string {
  if (kind) return kind;
  const l = normLang(lang);
  return l === "markdown" ? "document" : l === "html" ? "page" : l === "react" ? "component" : "schema";
}

export const KIND_LABEL: Record<string, string> = {
  document: "Document",
  slides: "Diapositives",
  design: "Design",
  dashboard: "Tableau de bord",
  page: "Page web",
  schema: "Schéma",
  component: "Composant",
};
