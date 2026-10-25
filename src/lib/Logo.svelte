<script lang="ts">
  /** Le lama de l'application (même dessin que l'icône). `plain` : sans le carré orange, pour un fond déjà coloré. */
  let { size = 28, plain = false, title = "", animated = false }: { size?: number; plain?: boolean; title?: string; animated?: boolean } = $props();

  const CREAM = "#faf1e6";
  const MUZZLE = "#ecd9c6";
  const EAR = "#f0b8a0";
  const DARK = "#3b2a22";
</script>

<svg class:animated viewBox="0 0 100 100" width={size} height={size} role={title ? "img" : undefined} aria-hidden={title ? undefined : "true"} aria-label={title || undefined} focusable="false">
  {#if !plain}
    <defs>
      <linearGradient id="lama-bg" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#de7650" />
        <stop offset="1" stop-color="#b4502e" />
      </linearGradient>
      <clipPath id="lama-clip"><rect width="100" height="100" rx="22" /></clipPath>
    </defs>
    <rect width="100" height="100" rx="22" fill="url(#lama-bg)" />
  {/if}
  <g clip-path={plain ? undefined : "url(#lama-clip)"}>
    <polygon points="40,56 60,56 72,100 28,100" fill={CREAM} />
    <ellipse cx="33.5" cy="25" rx="4.8" ry="11.5" transform="rotate(-24 33.5 25)" fill={CREAM} />
    <ellipse cx="66.5" cy="25" rx="4.8" ry="11.5" transform="rotate(24 66.5 25)" fill={CREAM} />
    <ellipse cx="33.8" cy="25.6" rx="2.2" ry="7.2" transform="rotate(-24 33.8 25.6)" fill={EAR} />
    <ellipse cx="66.2" cy="25.6" rx="2.2" ry="7.2" transform="rotate(24 66.2 25.6)" fill={EAR} />
    <ellipse cx="50" cy="44" rx="16.5" ry="19.5" fill={CREAM} />
    <circle cx="42.5" cy="28.5" r="6.2" fill={CREAM} />
    <circle cx="50" cy="26" r="7.2" fill={CREAM} />
    <circle cx="57.5" cy="28.5" r="6.2" fill={CREAM} />
    <ellipse cx="50" cy="56" rx="10.8" ry="8.8" fill={MUZZLE} />
    <g class="eyes">
      <ellipse cx="42.5" cy="41" rx="2.7" ry="3" fill={DARK} />
      <ellipse cx="57.5" cy="41" rx="2.7" ry="3" fill={DARK} />
    </g>
    <circle cx="43.3" cy="40" r="0.9" fill="#fff" />
    <circle cx="58.3" cy="40" r="0.9" fill="#fff" />
    <ellipse cx="50" cy="53.2" rx="3.4" ry="2.3" fill={DARK} />
    <path d="M50 55.4V57M45.6 57.2Q47.8 61.6 50 57.2M50 57.2Q52.2 61.6 54.4 57.2" stroke={DARK} stroke-width="0.9" stroke-linecap="round" fill="none" />
  </g>
</svg>

<style>
  svg { flex: none; display: block; }
  /* Le lama arrive avec un petit rebond, puis cligne des yeux de temps en temps. */
  svg.animated { animation: arrive 0.8s cubic-bezier(0.3, 1.5, 0.5, 1) both; }
  svg.animated .eyes { transform-box: fill-box; transform-origin: center; animation: blink 5s 1.4s infinite; }
  @keyframes arrive { from { opacity: 0; transform: scale(0.55) rotate(-8deg); } to { opacity: 1; transform: none; } }
  @keyframes blink { 0%, 94%, 100% { transform: scaleY(1); } 97% { transform: scaleY(0.08); } }
</style>
