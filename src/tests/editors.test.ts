import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { ChatStore } from "$lib/chat.svelte";
import SkillEditor from "$lib/SkillEditor.svelte";
import AgentEditor from "$lib/AgentEditor.svelte";

async function store() {
  const chat = new ChatStore("demo-key");
  await chat.loadSkills();
  await chat.loadAgents();
  return chat;
}

const fill = async (el: HTMLElement, value: string) => fireEvent.input(el, { target: { value } });

describe("éditeur de skills", () => {
  it("ne propose d'enregistrer que quand tout est rempli, et dit ce qui manque", async () => {
    const chat = await store();
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved: () => {} });
    const create = screen.getByRole("button", { name: "Créer le skill" });
    expect(create).toBeDisabled();

    await fill(screen.getByPlaceholderText("ex. relire-un-texte"), "Mon skill");
    expect(screen.getByText("Commande : /mon-skill")).toBeInTheDocument();
    expect(create).toBeDisabled();

    await fill(screen.getByPlaceholderText(/Utilise ce skill quand on demande/), "Utilise ce skill quand on veut un exemple.");
    expect(create).toBeDisabled();
    await fill(screen.getByPlaceholderText(/# Objectif/), "# Objectif\n\nFaire quelque chose d'utile.");
    expect(create).toBeEnabled();
  });

  it("remplit les champs depuis un modèle de départ", async () => {
    const chat = await store();
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Résumer un document" }));
    expect((screen.getByPlaceholderText("ex. relire-un-texte") as HTMLInputElement).value).toBe("resumer");
    expect(screen.getByRole("button", { name: "Créer le skill" })).toBeEnabled();
  });

  it("refuse un nom déjà pris", async () => {
    const chat = await store();
    const existing = chat.skills[0].name;
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved: () => {} });
    await fill(screen.getByPlaceholderText("ex. relire-un-texte"), existing);
    expect(screen.getByText(`« ${existing} » existe déjà.`)).toBeInTheDocument();
  });

  it("enregistre, recharge la liste et prévient", async () => {
    const chat = await store();
    const onsaved = vi.fn();
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved });
    await fireEvent.click(screen.getByRole("button", { name: "Relire un texte" }));
    await fireEvent.click(screen.getByRole("button", { name: "Créer le skill" }));
    await waitFor(() => expect(onsaved).toHaveBeenCalled());
    expect(onsaved.mock.calls[0][0]).toContain("/relire-un-texte");
    expect(chat.skills.some((s) => s.name === "relire-un-texte")).toBe(true);
  });

  it("rédige avec Lama à partir d'une idée, même si la réponse est entourée de texte", async () => {
    const chat = await store();
    chat.model = "gpt-oss:120b";
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved: () => {} });
    await fill(screen.getByLabelText("Ton idée de skill"), "répondre aux mails de mes clients");
    await fireEvent.click(screen.getByRole("button", { name: "Rédiger avec Lama" }));
    await waitFor(() => expect((screen.getByPlaceholderText("ex. relire-un-texte") as HTMLInputElement).value).toBe("repondre-aux-clients"));
  });

  it("demande de choisir un modèle avant de rédiger", async () => {
    const chat = await store();
    chat.model = "";
    render(SkillEditor, { chat, editing: null, onclose: () => {}, onsaved: () => {} });
    await fill(screen.getByLabelText("Ton idée de skill"), "une idée");
    await fireEvent.click(screen.getByRole("button", { name: "Rédiger avec Lama" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choisis d'abord un modèle");
  });

  it("charge un skill existant pour le modifier", async () => {
    const chat = await store();
    const name = chat.skills[0].name;
    render(SkillEditor, { chat, editing: name, onclose: () => {}, onsaved: () => {} });
    await waitFor(() => expect((screen.getByPlaceholderText("ex. relire-un-texte") as HTMLInputElement).value).toBe(name));
    expect(screen.getByRole("button", { name: "Enregistrer" })).toBeInTheDocument();
    expect(screen.queryByText("Pour commencer")).not.toBeInTheDocument();
  });
});

describe("éditeur d'agents", () => {
  it("copie un agent intégré sous un autre nom", async () => {
    const chat = await store();
    const builtin = chat.agents.find((a) => a.scope === "builtin")!;
    const onsaved = vi.fn();
    render(AgentEditor, { chat, from: builtin.name, copy: true, onclose: () => {}, onsaved });
    await waitFor(() => expect((screen.getByPlaceholderText("ex. relecteur-securite") as HTMLInputElement).value).toBe(`${builtin.name}-perso`));
    await fireEvent.click(screen.getByRole("button", { name: "Créer l'agent" }));
    await waitFor(() => expect(onsaved).toHaveBeenCalled());
    expect(chat.agents.some((a) => a.name === `${builtin.name}-perso`)).toBe(true);
  });

  it("traduit le niveau d'accès choisi en liste d'outils", async () => {
    const chat = await store();
    render(AgentEditor, { chat, from: null, onclose: () => {}, onsaved: () => {} });
    await fireEvent.click(screen.getByRole("button", { name: "Relecteur sécurité" }));
    expect(screen.getByRole("radio", { name: "Lecture seule" })).toHaveAttribute("aria-checked", "true");
    await fireEvent.click(screen.getByRole("radio", { name: "Lecture et web" }));
    expect(screen.getByRole("radio", { name: "Lecture et web" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Comme la lecture seule, avec la recherche sur internet.")).toBeInTheDocument();
  });

  it("refuse un nom déjà pris", async () => {
    const chat = await store();
    render(AgentEditor, { chat, from: null, onclose: () => {}, onsaved: () => {} });
    await fill(screen.getByPlaceholderText("ex. relecteur-securite"), chat.agents[0].name);
    expect(screen.getByText(`« ${chat.agents[0].name} » existe déjà.`)).toBeInTheDocument();
  });
});

describe("rédaction assistée des tâches et des projets", () => {
  it("une tâche planifiée est rédigée à partir d'une phrase", async () => {
    const chat = await store();
    chat.model = "gpt-oss:120b";
    const { default: TasksPage } = await import("$lib/TasksPage.svelte");
    render(TasksPage, { chat });
    await fireEvent.click(screen.getByRole("button", { name: /Nouvelle tâche|Nouvelle/ }));
    await fill(screen.getByLabelText("Ton idée, en une phrase"), "chaque matin, résume-moi les nouveautés sur Tauri");
    await fireEvent.click(screen.getByRole("button", { name: "Rédiger avec Lama" }));
    await waitFor(() => expect((screen.getByPlaceholderText("Ex. Revue de presse du matin") as HTMLInputElement).value).toBe("Revue de presse Tauri"));
    expect((screen.getByPlaceholderText(/Ex. Cherche sur le web/) as HTMLTextAreaElement).value).toContain("résume-les en 5 points");
  });

  it("un projet reçoit un nom et des instructions, sans écraser un nom déjà tapé", async () => {
    const chat = await store();
    chat.model = "gpt-oss:120b";
    const { default: ProjectDialog } = await import("$lib/ProjectDialog.svelte");
    render(ProjectDialog, { chat, project: null, code: false, onsave: () => {}, oncancel: () => {} });
    await fill(screen.getByPlaceholderText("Ex. Application mobile"), "Mon nom à moi");
    await fill(screen.getByLabelText("Ton idée, en une phrase"), "le suivi de mon mémoire de master en histoire");
    await fireEvent.click(screen.getByRole("button", { name: "Rédiger avec Lama" }));
    await waitFor(() => expect((screen.getByLabelText(/Instructions/) as HTMLTextAreaElement).value).toContain("mémoire de master"));
    expect((screen.getByPlaceholderText("Ex. Application mobile") as HTMLInputElement).value).toBe("Mon nom à moi");
  });

  it("sans modèle choisi, la rédaction explique quoi faire", async () => {
    const chat = await store();
    chat.model = "";
    const { default: ProjectDialog } = await import("$lib/ProjectDialog.svelte");
    render(ProjectDialog, { chat, project: null, code: false, onsave: () => {}, oncancel: () => {} });
    await fill(screen.getByLabelText("Ton idée, en une phrase"), "une idée");
    await fireEvent.click(screen.getByRole("button", { name: "Rédiger avec Lama" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choisis d'abord un modèle");
  });
});
