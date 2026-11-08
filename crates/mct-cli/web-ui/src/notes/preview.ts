import { computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { NoteContext } from "./dom";

const DELAY_MS = 250;
let counter = 0;

/**
 * Shows a short description of a wikilink's note while the link is hovered or
 * focused, positioned by Floating UI. The panel is a plain tooltip: it never
 * takes focus or pointer events, so it cannot get in the way of the link.
 * Nothing is shown for a note the context cannot describe.
 */
export function attachPreview(
  link: HTMLElement,
  name: string,
  preview: NonNullable<NoteContext["preview"]>,
): void {
  const doc = link.ownerDocument;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let panel: HTMLElement | undefined;
  // Bumped on every show/hide, so a slow answer for an earlier hover is dropped.
  let generation = 0;

  const hide = () => {
    generation += 1;
    clearTimeout(timer);
    panel?.remove();
    panel = undefined;
    link.removeAttribute("aria-describedby");
    doc.removeEventListener("keydown", onKey, true);
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") hide();
  };

  const show = async () => {
    const mine = ++generation;
    let info: Awaited<ReturnType<typeof preview>>;
    try {
      info = await preview(name);
    } catch {
      return;
    }
    if (mine !== generation || !info || !link.isConnected) return;
    const id = `note-preview-${++counter}`;
    const title = doc.createElement("strong");
    title.textContent = info.title;
    const text = doc.createElement("span");
    text.textContent = info.excerpt;
    panel = doc.createElement("div");
    panel.id = id;
    panel.className = "note-preview";
    panel.setAttribute("role", "tooltip");
    panel.append(title, text);
    doc.body.append(panel);
    link.setAttribute("aria-describedby", id);
    doc.addEventListener("keydown", onKey, true);
    const shown = panel;
    const { x, y } = await computePosition(link, shown, {
      placement: "bottom-start",
      middleware: [offset(6), flip(), shift({ padding: 8 })],
    });
    if (shown !== panel) return;
    shown.style.left = `${x}px`;
    shown.style.top = `${y}px`;
  };

  const schedule = (event: Event) => {
    // Touch has no hover: a tap opens the note and a preview would only cover it.
    if ((event as PointerEvent).pointerType === "touch") return;
    clearTimeout(timer);
    timer = setTimeout(() => void show(), DELAY_MS);
  };

  link.addEventListener("pointerenter", schedule);
  link.addEventListener("focus", schedule);
  link.addEventListener("pointerleave", hide);
  link.addEventListener("blur", hide);
  link.addEventListener("click", hide);
}
