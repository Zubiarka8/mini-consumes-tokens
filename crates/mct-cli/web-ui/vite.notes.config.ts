import { defineConfig } from "vite";

// The notes renderer is plain TypeScript (no React): one ES module with a fixed
// name. assets.rs embeds it with include_str!, so the Rust build never needs
// Node. The dashboard's page-notes.js `import()`s it for `renderNote`. It is
// built like review.js (an entry with exports, not Vite's library mode, which
// leaves ES output unminified).
export default defineConfig({
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    outDir: "../src/web/notes-dist",
    emptyOutDir: true,
    target: "es2022",
    modulePreload: false,
    reportCompressedSize: false,
    rollupOptions: {
      input: "src/notes/index.ts",
      preserveEntrySignatures: "exports-only",
      output: {
        format: "es",
        entryFileNames: "notes.js",
        codeSplitting: false,
      },
    },
  },
});
