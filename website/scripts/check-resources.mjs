import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import vm from "node:vm";
import { createInstance } from "i18next";
function resource(path) {
  const source = readFileSync(path, "utf8");
  const output = stripTypeScriptTypes(source).replace(
    /export default (en|es);/,
    "$1;",
  );
  return vm.runInNewContext(output);
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
