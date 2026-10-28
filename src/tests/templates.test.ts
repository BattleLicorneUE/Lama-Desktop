import { describe, expect, it } from "vitest";
import { AGENT_TEMPLATES, ACCESS, READ_TOOLS, WEB_TOOLS, accessOf, parseAgentReply } from "$lib/agent-templates";
import { SKILL_TEMPLATES, parseSkillReply, slugify } from "$lib/skill-templates";
import { fmtBytes } from "$lib/chat.svelte";

describe("slugify", () => {
  it("fait un nom de commande sûr", () => {
    expect(slugify("  Relire un texte !  ")).toBe("relire-un-texte");
    expect(slugify("Révision__de--code")).toBe("révision-de-code");
    expect(slugify("../../etc")).toBe("etc");
    expect(slugify("a/b\\c:d")).toBe("a-b-c-d");
    expect(slugify("---")).toBe("");
  });
});

describe("réponse du modèle pour un skill", () => {
  const json = { name: "resumer", description: "Utilise ce skill quand on veut un résumé.", argument_hint: "[le texte]", body: "# Résumé\n\n1. Lis.\n2. Résume." };

  it("lit un objet JSON nu", () => {
    expect(parseSkillReply(JSON.stringify(json))).toEqual(json);
  });
  it("lit un objet entouré de texte et de ```", () => {
    const text = "Voici le skill :\n```json\n" + JSON.stringify(json) + "\n```\nBonne utilisation !";
    expect(parseSkillReply(text)?.name).toBe("resumer");
  });
  it("refuse ce qui n'est pas exploitable", () => {
    expect(parseSkillReply("Désolé, je ne peux pas.")).toBeNull();
    expect(parseSkillReply("{ pas du json }")).toBeNull();
    expect(parseSkillReply("{}")).toBeNull();
    expect(parseSkillReply("")).toBeNull();
  });
  it("ignore les champs du mauvais type", () => {
    const d = parseSkillReply('{"name":3,"description":"Utilise ce skill quand…","body":["x"]}');
    expect(d?.name).toBe("");
    expect(d?.body).toBe("");
  });
});

describe("modèles de skills", () => {
  it("respectent les règles de l'application (même validation que le Rust)", () => {
    for (const t of SKILL_TEMPLATES.filter((x) => x.id !== "blank")) {
      expect(slugify(t.draft.name), t.id).toBe(t.draft.name);
      expect(t.draft.description.length, t.id).toBeGreaterThanOrEqual(8);
      expect(t.draft.description.length, t.id).toBeLessThanOrEqual(500);
      expect(t.draft.body.trim().length, t.id).toBeGreaterThanOrEqual(10);
      expect(t.draft.description, t.id).toMatch(/quand/i);
    }
  });
  it("ont des identifiants uniques", () => {
    const ids = SKILL_TEMPLATES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("agents", () => {
  it("reconnaît le niveau d'accès d'une liste d'outils", () => {
    expect(accessOf(null)).toBe("all");
    expect(accessOf(READ_TOOLS)).toBe("read");
    expect(accessOf([...READ_TOOLS].reverse())).toBe("read");
    expect(accessOf(WEB_TOOLS)).toBe("web");
    expect(accessOf(["read_file", "run_command"])).toBe("custom");
    expect(ACCESS.map((a) => accessOf(a.tools))).toEqual(["all", "read", "web"]);
  });
  it("lit la réponse du modèle", () => {
    const d = parseAgentReply('Voilà :\n{"name":"veilleur","description":"Confie-lui une recherche.","prompt":"Tu es un veilleur."}');
    expect(d).toEqual({ name: "veilleur", description: "Confie-lui une recherche.", prompt: "Tu es un veilleur." });
    expect(parseAgentReply("rien")).toBeNull();
  });
  it("les modèles d'agents sont complets et leurs outils connus", () => {
    for (const t of AGENT_TEMPLATES.filter((x) => x.id !== "blank")) {
      expect(slugify(t.draft.name), t.id).toBe(t.draft.name);
      expect(t.draft.description.length).toBeGreaterThanOrEqual(8);
      expect(t.draft.prompt.length).toBeGreaterThanOrEqual(10);
      for (const tool of t.draft.tools ?? []) expect(WEB_TOOLS).toContain(tool);
    }
  });
});

describe("tailles", () => {
  it("fmtBytes", () => {
    expect(fmtBytes(5_027_783_488)).toBe("4,7 Go");
    expect(fmtBytes(700 * 1024 * 1024)).toBe("700 Mo");
    expect(fmtBytes(10)).toBe("1 Mo");
  });
});
