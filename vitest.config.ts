import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// Tests de l'interface : les composants tournent dans un faux navigateur (happy-dom), le backend Tauri est remplacé par `src/tests/tauri.ts`.
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: { $lib: fileURLToPath(new URL("./src/lib", import.meta.url)) },
    // Svelte doit charger sa version « navigateur » pour que les composants se montent.
    conditions: ["browser"],
  },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/tests/setup.ts"],
    css: false,
  },
});
