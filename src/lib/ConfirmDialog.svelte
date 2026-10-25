<script lang="ts">
  import { answerConfirm, confirmState } from "$lib/dialogs.svelte";

  let okButton = $state<HTMLButtonElement>();
  $effect(() => {
    if (confirmState.current) okButton?.focus();
  });
</script>

<svelte:window onkeydown={(e) => confirmState.current && e.key === "Escape" && answerConfirm(false)} />

{#if confirmState.current}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay top" onmousedown={(e) => e.target === e.currentTarget && answerConfirm(false)}>
    <div class="dialog small" role="alertdialog" aria-modal="true" aria-label="Confirmation">
      <p>{confirmState.current.message}</p>
      <div class="row">
        <button onclick={() => answerConfirm(false)}>Annuler</button>
        <button class="danger" bind:this={okButton} onclick={() => answerConfirm(true)}>{confirmState.current.ok}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .small { max-width: 420px; gap: 16px; }
  p { margin: 0; line-height: 1.5; overflow-wrap: anywhere; white-space: pre-line; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { border: 1px solid var(--border); background: var(--surface); border-radius: 9px; padding: 7px 16px; }
  button:hover { border-color: var(--accent); }
  .danger { background: var(--del); border-color: var(--del); color: var(--bg); font-weight: 600; }
  .danger:hover { filter: brightness(1.08); border-color: var(--del); }
</style>
