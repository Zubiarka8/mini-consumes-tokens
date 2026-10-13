import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, resolve } from "node:path";
import vm from "node:vm";
import { createInstance } from "i18next";
function resource(path) {
  const dir = dirname(path);
  const strip = (file) => stripTypeScriptTypes(readFileSync(file, "utf8"));
  const parts = [];
  const main = strip(path);
  for (const [, from] of main.matchAll(/import \{[^}]*\} from "([^"]+)";/g))
    parts.push(
      strip(resolve(dir, from + ".ts"))
        .replace(/^import .*$/gm, "")
        .replace(/export const/g, "const"),
    );
  parts.push(
    main.replace(/^import .*$/gm, "").replace(/export default (en|es);/, "$1;"),
  );
  return vm.runInNewContext(parts.join("\n"));
}
const en = resource("messages/en.ts");
const es = resource("messages/es.ts");
assert.deepEqual(Object.keys(es).sort(), Object.keys(en).sort());
for (const [language, entries] of Object.entries({ en, es }))
  for (const [key, value] of Object.entries(entries))
    assert.ok(value.trim(), `${language}.${key} is empty`);
const i18n = createInstance();
await i18n.init({
  resources: { en: { translation: en }, es: { translation: es } },
  lng: "en",
  fallbackLng: "en",
  initAsync: false,
});
assert.equal(i18n.t("headline"), "Less searching.");
await i18n.changeLanguage("es");
assert.equal(i18n.t("headline"), "Menos búsquedas.");
await i18n.changeLanguage("invalid");
assert.equal(i18n.t("headline"), "Less searching.");
console.log(
  `PASS: ${Object.keys(en).length} matching, nonempty translation keys; real i18next switching and fallback`,
);
