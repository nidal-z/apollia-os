import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "node:path";

export default defineConfig(({ command }) => ({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "src/lib"),
      // Dev server only: the automation runner's stubInvoke step answers IPC
      // calls through this shim, since the webview's own invoke is read-only.
      // A production build resolves the real module.
      ...(command === "serve"
        ? { "@tauri-apps/api/core": path.resolve(__dirname, "src/lib/automation/tauriCoreShim.ts") }
        : {}),
    },
  },
  // The Tauri plugins import `@tauri-apps/api/core` too; pre-bundled, they
  // would carry their own copy of it and bypass the alias above (measured:
  // a stubbed `plugin:dialog|open` was never consumed). Served as source in
  // the dev server, their imports resolve through the alias like the app's.
  optimizeDeps: {
    exclude: command === "serve" ? ["@tauri-apps/plugin-dialog", "@tauri-apps/plugin-notification", "@tauri-apps/plugin-opener"] : [],
  },
  server: {
    // The literal IPv4 loopback rather than `localhost`: Node binds the name
    // to one loopback only (`::1` since Node 17), and a WebView2 profile that
    // resolves `localhost` to 127.0.0.1 then gets ERR_CONNECTION_REFUSED and
    // a blank window, with nothing in the application's journal. Measured on
    // 2026-09-10 on Windows with a fresh profile. The desktop's `devUrl`
    // names the same literal, so nothing is resolved on either side.
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "esnext",
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        overlay: path.resolve(__dirname, "overlay.html"),
      },
    },
  },
}));
