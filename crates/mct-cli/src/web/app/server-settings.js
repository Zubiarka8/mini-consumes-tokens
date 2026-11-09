// Keeps the choices made in Settings in the project's `.mct-index/web-settings.json` as well as in the
// browser, so they survive cleared site data, another browser and a restarted server. A choice this
// browser has wins and is written to the file; one it has never made is taken from the file.
import { api, store, applyTheme, saveSettings } from "./core.js";
import { power } from "./power.js";
import { getLang, setLang, hasSavedLang } from "./i18n.js";

/** Saves `patch` in the file; the page keeps working when the server cannot be reached. */
export const persist = (patch) => saveSettings(patch).catch(() => {});

export async function syncSettings() {
  let file;
  try {
    file = await api("settings");
  } catch {
    return;
  }
  const push = {};
  const theme = store.get("theme", null);
  if (theme) {
    if (file.theme !== theme) push.theme = theme;
  } else if (file.theme) {
    store.set("theme", file.theme);
    applyTheme(file.theme);
  }
  for (const name of ["stop", "pause"]) {
    const own = power.saved(name);
    if (own !== null) {
      if (file[name] !== own) push[name] = own;
    } else if (Number.isInteger(file[name])) power.set(name, file[name]);
  }
  if (hasSavedLang()) {
    if (file.lang !== getLang()) push.lang = getLang();
  } else if (file.lang) {
    // Reloads once; the saved choice is then this browser's own.
    setLang(file.lang);
    return;
  }
  if (Object.keys(push).length) await persist(push);
}
