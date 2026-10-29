import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Sonner adds its stylesheet with an injected <style> element, which the
// dashboard's `style-src 'self'` policy blocks. styles.css imports the same
// rules into review.css, so the injection is removed from the bundle.
const sonnerWithoutInjectedStyle = (): Plugin => ({
  name: "sonner-without-injected-style",
  transform(code, id) {
    if (!/[\\/]sonner[\\/]dist[\\/]index\.mjs$/.test(id)) return null;
    const stripped = code.replace(/^__insertCSS\(".*"\);$/m, "");
    if (stripped === code) this.error("sonner's style injection was not found; update this plugin");
    return { code: stripped, map: null };
  },
});

// One ES module and one stylesheet with fixed names: assets.rs embeds them
// with include_str!, so the Rust build never needs Node.
export default defineConfig({
  plugins: [react(), tailwindcss(), sonnerWithoutInjectedStyle()],
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    outDir: "../src/web/review-dist",
    emptyOutDir: true,
    target: "es2022",
    modulePreload: false,
    cssCodeSplit: false,
    reportCompressedSize: false,
    rollupOptions: {
      input: "src/main.tsx",
      preserveEntrySignatures: "exports-only",
      output: {
        format: "es",
        entryFileNames: "review.js",
        assetFileNames: "review[extname]",
        codeSplitting: false,
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["src/test-setup.ts"],
    css: false,
  },
});
