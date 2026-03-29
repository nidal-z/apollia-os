import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, "src/lib"),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
