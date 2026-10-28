// Starts the real dashboard against the frozen snapshot: installs a `fetch`
// that answers `api/...` from backend.js, then loads the unmodified app.
import { createBackend } from "./backend.js";

const snapshot = await (await fetch("snapshot.json")).json();
const handle = createBackend(snapshot);
const realFetch = window.fetch.bind(window);

window.fetch = async (input, init = {}) => {
  const url = typeof input === "string" ? input : input.url;
  const answer = handle(url, (init.method || "GET").toUpperCase(), init.body);
  if (!answer) return realFetch(input, init);
  return new Response(JSON.stringify(answer.body), { status: answer.status, headers: { "Content-Type": "application/json" } });
};

const badge = document.createElement("a");
badge.className = "demo-badge";
badge.href = "https://github.com/Zubiarka8/mini-consumes-tokens";
badge.textContent = "Demo · frozen snapshot of this repository · decisions are not saved";
document.body.append(badge);

await import("./js/main.js");
