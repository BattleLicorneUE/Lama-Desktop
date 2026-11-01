import { listen } from "@tauri-apps/api/event";
import { urlForTool } from "$lib/browser";
import { isWrite } from "$lib/chat-types";
import type { ChatStore } from "$lib/chat.svelte";
import type { LocalProgress, Task, ToolEvent } from "$lib/chat-types";
import type { ScheduledTask } from "$lib/schedule";

/** Les événements que le cœur Rust envoie à l'interface (réponses, outils, modèles locaux, tâches planifiées…). Renvoie de quoi s'en désabonner. */
export function listenToBackend(store: ChatStore): Promise<Array<() => void>> {
  return Promise.all([
      listen<{ id: string; chunk: any }>("chat:chunk", async (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        store.queueChunk(s, e.payload.chunk.content ?? "", e.payload.chunk.thinking ?? "");
        if (!e.payload.chunk.done) return;
        store.flushChunks(s);
        s.busy = false;
        await store.persist(s);
        const last = s.messages[s.messages.length - 1];
        void store.saveArtifact(s, last?.content ?? "");
        const text = (last?.content ?? "").replace(/\s+/g, " ").trim() || "Lama Desktop";
        if (s !== store.fg) {
          // Conversation restée en arrière-plan : elle a fini pendant qu'on regardait autre chose.
          store.unpark(s);
          const title = store.titleOf(s);
          store.alert(`Terminé : ${title}`, text);
          store.track({ kind: "answer" });
          store.flash(`« ${title} » a terminé.`);
          return;
        }
        store.openArtifact(last?.content ?? "");
        store.track({ kind: "answer" });
        if (store.mode === "code" && store.sessionChanges > store.changesBeforeRun) store.track({ kind: "code" });
        if (store.mode === "code" && store.autoReview && store.sessionChanges > store.changesBeforeRun) void store.reviewChanges(true);
        store.alert(store.mode === "code" ? "Tâche terminée" : "Réponse prête", text);
        // Une précision arrivée trop tard (ou pour un modèle sans outils) part comme un message normal.
        if (s.queued.length) {
          const next = s.queued.join("\n\n");
          s.queued = [];
          void store.send(next);
        }
      }),
      listen<{ id: string; text: string }>("agent:injected", async (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        const i = s.queued.indexOf(e.payload.text);
        if (i >= 0) s.queued.splice(i, 1);
        store.flushChunks(s);
        const cur = s.messages[s.messages.length - 1];
        if (cur?.role === "assistant" && !cur.content && !cur.thinking && !cur.tools?.length) s.messages.pop();
        s.messages.push({ role: "user", content: e.payload.text });
        s.messages.push({ role: "assistant", content: "" });
        if (s === store.fg) await store.scrollDown();
      }),
      listen<{ id: string }>("agent:step", (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        store.flushChunks(s);
        const last = s.messages[s.messages.length - 1];
        if (last?.role !== "assistant") return;
        // Le texte de l'étape qui vient de finir devient un commentaire, placé avant les outils qu'elle a lancés.
        if (last.content.trim()) {
          last.notes ??= [];
          last.notes.push({ text: last.content.trim(), at: last.stepStart ?? 0 });
          last.content = "";
        }
        last.stepStart = last.tools?.length ?? 0;
      }),
      listen("emergency:stop", () => {
        // Arrêt d'urgence (raccourci global ou icône de la zone de notification) : toutes les conversations s'interrompent.
        for (const s of [store.fg, ...store.parked]) {
          store.dropChunks(s);
          s.busy = false;
          const cut = s.messages[s.messages.length - 1];
          if (cut?.role === "assistant") cut.interrupted = true;
          s.messages.forEach((m) => m.tools?.forEach((t) => {
            if (t.state === "awaiting" || t.state === "running") t.state = "denied";
          }));
          if (s !== store.fg) void store.persist(s);
        }
        store.parked = [];
        store.returnQueued();
        store.flash("Arrêt d'urgence : tout est arrêté.");
      }),
      listen<{ id?: string; prompt: number; eval: number }>("usage:tokens", (e) => {
        // Les événements sont diffusés à toutes les fenêtres : on ne compte que ceux de ses propres requêtes.
        const s = e.payload.id ? store.sess(e.payload.id) : undefined;
        if (s) store.addUsage(e.payload.prompt, e.payload.eval, s.model);
      }),
      listen<LocalProgress>("local:progress", (e) => {
        store.trackRate(e.payload);
        store.localProgress = e.payload;
      }),
      listen<{ id: string }>("local:done", async (e) => {
        store.localProgress = null;
        store.localRate = 0;
        await store.loadLocal();
        const name = store.localInstalled.find((m) => m.id === e.payload.id)?.name ?? e.payload.id;
        store.localNotice = `${name} est installé.`;
        await store.startNextLocal();
      }),
      listen<{ id: string; error: string }>("local:error", async (e) => {
        store.localProgress = null;
        store.localRate = 0;
        if (/annulée/i.test(e.payload.error)) store.localNotice = "Installation annulée.";
        else store.localError = e.payload.error;
        await store.startNextLocal();
      }),
      listen<ScheduledTask>("schedule:due", (e) => void store.runTask(e.payload)),
      listen<{ id: string; prompt_tokens: number }>("chat:usage", (e) => {
        const s = store.sess(e.payload.id);
        if (s) s.promptTokens = e.payload.prompt_tokens;
      }),
      listen<{ id: string; error: string }>("chat:error", async (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        store.flushChunks(s);
        s.error = e.payload.error;
        s.busy = false;
        const failed = s.messages[s.messages.length - 1];
        if (failed?.role === "assistant") failed.interrupted = true;
        if (s === store.fg) store.returnQueued();
        else {
          store.unpark(s);
          store.alert(`Erreur : ${store.titleOf(s)}`, e.payload.error);
        }
        await store.persist(s);
      }),
      listen<{ id: string; tasks: Task[] }>("agent:tasks", async (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        s.messages[s.messages.length - 1].tasks = e.payload.tasks;
        if (s === store.fg) await store.scrollDown();
      }),
      listen<ToolEvent & { id: string }>("agent:tool", async (e) => {
        const s = store.sess(e.payload.id);
        if (!s) return;
        const last = s.messages[s.messages.length - 1];
        const { id: _id, ...ev } = e.payload;
        last.tools ??= [];
        const i = last.tools.findIndex((t) => t.call_id === ev.call_id);
        if (ev.state === "done" && isWrite(ev.name) && last.tools[i]?.state !== "done") store.sessionChanges++;
        if (ev.state === "awaiting") store.alert(s === store.fg ? "Autorisation requise" : `Autorisation requise : ${store.titleOf(s)}`, `${ev.name} ${String(ev.args.path ?? ev.args.command ?? ev.args.to ?? "")}`);
        if (i >= 0) last.tools[i] = ev;
        else last.tools.push(ev);
        // Quand Lama cherche ou lit une page, on la montre : une seule fois par appel, et jamais pour une action refusée.
        if (store.browserAuto && s === store.fg && (ev.state === "running" || ev.state === "done") && !store.browsed.has(ev.call_id)) {
          const page = urlForTool(ev.name, ev.args);
          if (page) {
            store.browsed.add(ev.call_id);
            store.openBrowser(page);
          }
        }
        store.autosave(s);
        if (s === store.fg) await store.scrollDown();
      }),
  ]);
}
