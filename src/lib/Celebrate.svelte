<script lang="ts">
  import { fly } from "svelte/transition";
  import type { ChatStore } from "$lib/chat.svelte";
  import Icon from "$lib/Icon.svelte";

  let { chat }: { chat: ChatStore } = $props();
  const current = $derived(chat.celebrations[0]);

  // Un message à la fois, qui se retire seul.
  $effect(() => {
    const c = current;
    if (!c) return;
    const t = setTimeout(() => chat.dismissCelebration(c.id), c.kind === "level" ? 5600 : 4400);
    return () => clearTimeout(t);
  });

  const COLORS = ["#de7650", "#f0b8a0", "#faf1e6", "#e8b04a", "#6bbf8a", "#6aa0e8", "#c58ae0"];
  /** Confettis : tirés une fois par message, qui jaillissent de l'icône puis retombent. */
  const pieces = $derived.by(() => {
    const c = current;
    if (!c) return [];
    let seed = c.id * 9301 + 49297;
    const rnd = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280);
    return Array.from({ length: c.kind === "level" ? 44 : 24 }, (_, i) => ({
      dx: Math.round((rnd() - 0.5) * (c.kind === "level" ? 360 : 240)),
      dy: Math.round(-60 - rnd() * 130),
      r: Math.round((rnd() - 0.5) * 900),
      d: Math.round(rnd() * 160),
      s: 6 + Math.round(rnd() * 6),
      c: COLORS[i % COLORS.length],
      round: rnd() > 0.6,
    }));
  });
</script>

{#if current}
  {#key current.id}
    <div class="wrap" aria-live="polite">
      <button class="toast" class:level={current.kind === "level"} onclick={() => chat.openProgress()} in:fly={{ y: 40, duration: 450 }} out:fly={{ y: 20, duration: 250 }}>
        <span class="medal"><Icon name={current.icon} size={current.kind === "level" ? 26 : 22} />
          <span class="burst" aria-hidden="true">
            {#each pieces as p}
              <i style:--dx="{p.dx}px" style:--dy="{p.dy}px" style:--r="{p.r}deg" style:--d="{p.d}ms" style:--s="{p.s}px" style:background={p.c} class:round={p.round}></i>
            {/each}
          </span>
        </span>
        <span class="txt">
          <small>{current.kind === "level" ? "Niveau supérieur" : "Nouveau badge"}</small>
          <b>{current.title}</b>
          <span>{current.text}</span>
        </span>
      </button>
    </div>
  {/key}
{/if}

<style>
  :global(body[data-pane]) .wrap { right: calc(46% + 22px); }
  .wrap { position: fixed; right: 22px; bottom: 22px; z-index: 300; pointer-events: none; }
  .toast { pointer-events: auto; display: flex; align-items: center; gap: 14px; width: min(360px, calc(100vw - 44px)); text-align: left; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border)); background: var(--surface); color: var(--fg); border-radius: 16px; padding: 14px 16px; box-shadow: 0 18px 44px rgb(0 0 0 / 0.35); overflow: visible; }
  .toast.level { border-color: var(--accent); box-shadow: 0 18px 54px color-mix(in srgb, var(--accent) 40%, rgb(0 0 0 / 0.4)); }
  .medal { position: relative; flex: none; display: grid; place-items: center; width: 50px; height: 50px; border-radius: 50%; background: linear-gradient(160deg, #de7650, #b4502e); color: #fff; animation: medal 0.7s 0.15s cubic-bezier(0.3, 1.6, 0.5, 1) both; }
  .level .medal { width: 58px; height: 58px; animation: medal 0.8s 0.15s cubic-bezier(0.3, 1.6, 0.5, 1) both, glow 1.6s 0.9s ease-in-out 2; }
  .txt { display: grid; gap: 1px; min-width: 0; }
  .txt small { color: var(--accent); font-size: 12px; letter-spacing: 0.06em; text-transform: uppercase; }
  .txt b { font-size: 16px; font-weight: 600; }
  .txt span { color: var(--muted); font-size: 13.5px; line-height: 1.4; }
  .burst { position: absolute; left: 50%; top: 50%; width: 0; height: 0; }
  .burst i { position: absolute; left: 0; top: 0; width: var(--s); height: calc(var(--s) * 0.55); opacity: 0; border-radius: 1px; animation: confetti 1.5s var(--d) cubic-bezier(0.2, 0.7, 0.3, 1) forwards; }
  .burst i.round { border-radius: 50%; height: var(--s); }
  @keyframes confetti {
    0% { opacity: 1; transform: translate(0, 0) rotate(0) scale(0.4); }
    30% { opacity: 1; }
    100% { opacity: 0; transform: translate(var(--dx), calc(var(--dy) + 230px)) rotate(var(--r)) scale(1); }
  }
  @keyframes medal { from { transform: scale(0) rotate(-40deg); } to { transform: none; } }
  @keyframes glow { 50% { box-shadow: 0 0 0 10px color-mix(in srgb, var(--accent) 25%, transparent); } }
</style>
