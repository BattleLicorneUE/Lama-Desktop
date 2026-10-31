<script lang="ts">
  /** Jeu d'icônes unique (traits arrondis, couleur du texte) : remplace les emojis. */
  const PATHS: Record<string, string[]> = {
    folder: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
    globe: ["M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z", "M3 12h18", "M12 3c3 3.2 3 14.8 0 18", "M12 3c-3 3.2-3 14.8 0 18"],
    download: ["M12 4v11", "M7.5 11.5L12 16l4.5-4.5", "M5 19h14"],
    settings: ["M4 7h9", "M17 7h3", "M4 17h3", "M11 17h9", "M13 7a2 2 0 1 0 4 0a2 2 0 1 0-4 0", "M7 17a2 2 0 1 0 4 0a2 2 0 1 0-4 0"],
    shield: ["M12 3l7 3v5c0 4.5-3 8-7 10c-4-2-7-5.5-7-10V6z"],
    gauge: ["M12 4a8 8 0 1 0 8 8", "M12 12l4-4"],
    plus: ["M12 5v14", "M5 12h14"],
    search: ["M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z", "M16.5 16.5L21 21"],
    pin: ["M9 4h6l-1 6 3 3H7l3-3z", "M12 13v7"],
    mic: ["M9 5a3 3 0 0 1 6 0v6a3 3 0 0 1-6 0z", "M6 11a6 6 0 0 0 12 0", "M12 17v3", "M9 20h6"],
    pencil: ["M4 20h4L19 9l-4-4L4 16z"],
    trash: ["M5 7h14", "M10 7V4h4v3", "M7 7l1 13h8l1-13"],
    close: ["M6 6l12 12", "M18 6L6 18"],
    undo: ["M9 7L4 12l5 5", "M4 12h10a5 5 0 0 1 0 10h-2"],
    file: ["M7 3h7l5 5v13H7z", "M14 3v5h5"],
    chevron: ["M7 10l5 5l5-5"],
    sort: ["M8 20V5", "M4.5 8.5L8 5l3.5 3.5", "M16 4v15", "M12.5 15.5L16 19l3.5-3.5"],
    back: ["M15 5l-7 7l7 7"],
    book: ["M5 4h11a2 2 0 0 1 2 2v14H7a2 2 0 0 1-2-2z", "M5 18a2 2 0 0 1 2-2h11"],
    check: ["M5 12.5l4.5 4.5L19 7.5"],
    alert: ["M12 4l9 16H3z", "M12 10v4", "M12 17.5v.01"],
    clock: ["M12 4a8 8 0 1 0 0 16a8 8 0 1 0 0-16z", "M12 8v4l3 2"],
    play: ["M8 5l11 7-11 7z"],
    upload: ["M12 15V4", "M7.5 8.5L12 4l4.5 4.5", "M5 19h14"],
    flame: ["M12 3c.5 3.2 4.5 5.2 4.5 9.5a4.5 4.5 0 0 1-9 0c0-2 1-3.4 2-4.3.2 1.6 1 2.5 1.8 2.8C11 8.5 11.2 5.5 12 3z"],
    trophy: ["M8 4h8v5a4 4 0 0 1-8 0z", "M8 6H5v1a3 3 0 0 0 3 3", "M16 6h3v1a3 3 0 0 1-3 3", "M12 13v4", "M9 20h6", "M10 17h4"],
    star: ["M12 4l2.4 5 5.4.7-4 3.8 1 5.4L12 16.2l-4.8 2.7 1-5.4-4-3.8 5.4-.7z"],
    lock: ["M7 11h10v8H7z", "M9 11V8a3 3 0 0 1 6 0v3"],
    refresh: ["M20 11a8 8 0 1 0-2.3 5.7", "M20 5v6h-6"],
    "chevron-left": ["M14 7l-5 5l5 5"],
    "chevron-right": ["M10 7l5 5l-5 5"],
    external: ["M14 4h6v6", "M20 4l-9 9", "M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"],
  };

  let { name, size = 16 }: { name: string; size?: number } = $props();
</script>

<svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
  {#each PATHS[name] ?? [] as d}<path {d} />{/each}
</svg>

<style>
  svg { flex: none; vertical-align: -0.15em; }
</style>
