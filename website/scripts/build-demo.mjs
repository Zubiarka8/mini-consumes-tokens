// Assembles the public demo into public/demo/: the dashboard's own front end
// (crates/mct-cli/src/web/app), copied untouched, plus the demo shim and the
// frozen snapshot. Next copies public/ into the static export as-is.
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const website = join(dirname(fileURLToPath(import.meta.url)), "..");
const app = join(website, "..", "crates", "mct-cli", "src", "web", "app");
const demo = join(website, "demo");
const out = join(website, "public", "demo");

if (!existsSync(join(demo, "snapshot.json"))) {
  console.error(
    "website/demo/snapshot.json is missing; run scripts/export-demo-snapshot.mjs first",
  );
  process.exit(1);
}
rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "js"), { recursive: true });
cpSync(join(app, "app.css"), join(out, "app.css"));
for (const file of readdirSync(app).filter((name) => name.endsWith(".js")))
  cpSync(join(app, file), join(out, "js", file));
for (const file of [
  "index.html",
  "boot.js",
  "backend.js",
  "demo.css",
  "snapshot.json",
])
  cpSync(join(demo, file), join(out, file));
console.log(`demo assembled in ${out}`);
