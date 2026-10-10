import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";
const root = resolve("out");
const prefix = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
function files(directory) {
  return readdirSync(directory).flatMap((name) => {
    const file = join(directory, name);
    return statSync(file).isDirectory() ? files(file) : [file];
  });
}
const htmlFiles = files(root).filter((path) => path.endsWith(".html"));
let checked = 0;
for (const file of htmlFiles) {
  const html = readFileSync(file, "utf8");
  for (const match of html.matchAll(/(?:href|src)="([^"#]+)"/g)) {
    const url = match[1].split(/[?#]/)[0];
    if (!url.startsWith("/") || url.startsWith("//")) continue;
    assert.ok(
      !prefix || url === prefix || url.startsWith(prefix + "/"),
      `Missing prefix in ${file}: ${url}`,
    );
    const target = resolve(
      root,
      "." + decodeURIComponent(url.slice(prefix.length)),
    );
    assert.ok(
      existsSync(target) ||
        existsSync(target + ".html") ||
        existsSync(join(target, "index.html")),
      `Missing export in ${file}: ${url}`,
    );
    checked++;
  }
}
for (const page of [
  "index.html",
  "docs/index.html",
  "docs/installation/index.html",
  "docs/querying/index.html",
])
  assert.ok(existsSync(join(root, page)), page);
const home = readFileSync(join(root, "index.html"), "utf8");
assert.match(home, /Less searching\./);
assert.match(home, new RegExp(`href="${prefix}/docs/"`));
assert.equal((home.match(/<h1[ >]/g) ?? []).length, 1);
console.log(
  `PASS: ${htmlFiles.length} HTML exports, ${checked} local asset/link targets, homepage and direct docs routes (${prefix || "/"})`,
);
