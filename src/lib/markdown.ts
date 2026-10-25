import DOMPurify from "dompurify";
import katex from "katex";
import "katex/dist/katex.min.css";
import { Marked, type TokenizerAndRendererExtension } from "marked";
import { createHighlighter, type Highlighter } from "shiki";
import { isArtifact } from "$lib/artifacts";

const LANGS = [
  "javascript", "typescript", "python", "rust", "json", "bash", "powershell", "html", "css",
  "svelte", "sql", "yaml", "toml", "markdown", "c", "cpp", "csharp", "go", "java", "diff",
];

const tex = (src: string, displayMode: boolean) => katex.renderToString(src, { displayMode, throwOnError: false, output: "html" });

const mathBlock: TokenizerAndRendererExtension = {
  name: "mathBlock",
  level: "block",
  start: (src) => src.search(/\$\$|\\\[/),
  tokenizer(src) {
    const m = /^(?:\$\$\n?([\s\S]+?)\n?\$\$|\\\[([\s\S]+?)\\\])(?:\n|$)/.exec(src);
    if (m) return { type: "mathBlock", raw: m[0], text: (m[1] ?? m[2]).trim() };
  },
  renderer: (t) => `<div class="math">${tex(t.text as string, true)}</div>`,
};

const mathInline: TokenizerAndRendererExtension = {
  name: "mathInline",
  level: "inline",
  start: (src) => src.search(/\$|\\\(/),
  tokenizer(src) {
    const m = /^(?:\$(?!\s)([^\n$]+?)(?<!\s)\$(?!\d)|\\\((.+?)\\\))/.exec(src);
    if (m) return { type: "mathInline", raw: m[0], text: m[1] ?? m[2] };
  },
  renderer: (t) => tex(t.text as string, false),
};

const marked = new Marked({ gfm: true, breaks: true, extensions: [mathBlock, mathInline] });

// Une image ou un style qui va chercher quelque chose sur Internet s'enverrait avec l'adresse : c'est le moyen classique de faire
// fuiter une information après une injection dans un document. On ne garde que les images locales (données intégrées).
const LOCAL_SRC = /^(data:image\/|blob:|asset:|https?:\/\/asset\.localhost)/i;
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node instanceof Element && node.hasAttribute("style") && /url\s*\(|image-set\s*\(|@import|expression\s*\(/i.test(node.getAttribute("style") ?? "")) {
    node.removeAttribute("style");
  }
  if (node instanceof HTMLImageElement) {
    const src = node.getAttribute("src") ?? "";
    if (src && !LOCAL_SRC.test(src.trim())) {
      let host = "";
      try {
        host = new URL(src, "http://x.invalid").host.replace(/^x\.invalid$/, "");
      } catch {}
      node.replaceWith(document.createTextNode(`[image externe masquée${host ? " : " + host : ""}]`));
    }
    node.removeAttribute("srcset");
  }
});

/** Markdown -> HTML nettoyé (synchrone, utilisable pendant le streaming). */
export function renderMarkdown(src: string): string {
  return DOMPurify.sanitize(marked.parse(src, { async: false }) as string, { ADD_ATTR: ["style"] });
}

let highlighter: Promise<Highlighter> | undefined;
function getHighlighter() {
  highlighter ??= createHighlighter({ themes: ["github-light", "github-dark"], langs: LANGS });
  return highlighter;
}

/**
 * Action Svelte : colore les blocs de code et ajoute un bouton « Copier ».
 * Reçoit `{ html, final }` ; la coloration n'a lieu qu'une fois la réponse terminée.
 */
type EnhanceParams = { html: string; final: boolean; onpreview?: (code: string, lang: string) => void };

export function enhance(node: HTMLElement, params: EnhanceParams) {
  async function run(p: EnhanceParams) {
    if (!p.final) return;
    const hl = await getHighlighter();
    for (const pre of Array.from(node.querySelectorAll<HTMLPreElement>("pre"))) {
      if (pre.dataset.done) continue;
      pre.dataset.done = "1";
      const code = pre.querySelector("code");
      if (!code) continue;
      const text = code.textContent ?? "";
      const lang = /language-(\w+)/.exec(code.className)?.[1] ?? "";
      if (lang && hl.getLoadedLanguages().includes(lang)) {
        const tmp = document.createElement("div");
        tmp.innerHTML = hl.codeToHtml(text, {
          lang,
          themes: { light: "github-light", dark: "github-dark" },
          defaultColor: false,
        });
        const shiki = tmp.querySelector("pre");
        if (shiki) code.innerHTML = shiki.querySelector("code")?.innerHTML ?? code.innerHTML;
        pre.classList.add("shiki");
      }
      if (lang) {
        const tag = document.createElement("span");
        tag.className = "lang";
        tag.textContent = lang;
        pre.appendChild(tag);
      }
      const btn = document.createElement("button");
      btn.className = "copy";
      btn.textContent = "Copier";
      btn.onclick = async () => {
        await navigator.clipboard.writeText(text);
        btn.textContent = "Copié";
        setTimeout(() => (btn.textContent = "Copier"), 1500);
      };
      pre.appendChild(btn);
      if (p.onpreview && isArtifact(lang, text)) {
        const prev = document.createElement("button");
        prev.className = "copy preview";
        prev.textContent = "Aperçu";
        prev.onclick = () => p.onpreview?.(text, lang);
        pre.appendChild(prev);
      }
    }
  }
  run(params);
  return { update: run };
}
