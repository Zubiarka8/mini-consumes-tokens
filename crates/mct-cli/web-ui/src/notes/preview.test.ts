import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderNote } from "./index";

const hover = (link: Element, type = "pointerenter", pointerType = "mouse") => {
  const event = new Event(type, { bubbles: false });
  Object.defineProperty(event, "pointerType", { value: pointerType });
  link.dispatchEvent(event);
};
const settle = async () => {
  await vi.advanceTimersByTimeAsync(300);
};
const panel = () => document.querySelector(".note-preview");

function mount(
  preview:
    | ((name: string) => Promise<{ title: string; excerpt: string } | null>)
    | undefined,
) {
  const body = renderNote("See [[Other note]] here.", { preview });
  document.body.replaceChildren(body);
  return body.querySelector("a.wikilink")!;
}

describe("wikilink preview", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    document.body.replaceChildren();
  });

  it("shows the note's title and excerpt after a short hover, as a described tooltip", async () => {
    const preview = vi
      .fn()
      .mockResolvedValue({ title: "Other note", excerpt: "The linked note." });
    const link = mount(preview);
    hover(link);
    expect(panel()).toBeNull();
    await settle();
    expect(preview).toHaveBeenCalledWith("Other note");
    expect(panel()?.getAttribute("role")).toBe("tooltip");
    expect(panel()?.textContent).toBe("Other noteThe linked note.");
    expect(link.getAttribute("aria-describedby")).toBe(panel()?.id);
  });

  it("hides on leave, Escape, blur and click", async () => {
    const link = mount(vi.fn().mockResolvedValue({ title: "T", excerpt: "E" }));
    for (const close of [
      () => hover(link, "pointerleave"),
      () =>
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })),
      () => hover(link, "blur"),
      () => link.dispatchEvent(new MouseEvent("click", { cancelable: true })),
    ]) {
      hover(link);
      await settle();
      expect(panel()).not.toBeNull();
      close();
      expect(panel()).toBeNull();
      expect(link.hasAttribute("aria-describedby")).toBe(false);
    }
  });

  it("drops a slow answer when the pointer already left", async () => {
    let answer!: (value: { title: string; excerpt: string }) => void;
    const link = mount(() => new Promise((resolve) => (answer = resolve)));
    hover(link);
    await settle();
    hover(link, "pointerleave");
    answer({ title: "Late", excerpt: "answer" });
    await vi.advanceTimersByTimeAsync(10);
    expect(panel()).toBeNull();
  });

  it("shows nothing for an unknown note, a failed lookup or a touch", async () => {
    for (const preview of [
      vi.fn().mockResolvedValue(null),
      vi.fn().mockRejectedValue(new Error("offline")),
    ]) {
      hover(mount(preview));
      await settle();
      expect(panel()).toBeNull();
    }
    const preview = vi.fn().mockResolvedValue({ title: "T", excerpt: "E" });
    hover(mount(preview), "pointerenter", "touch");
    await settle();
    expect(preview).not.toHaveBeenCalled();
  });

  it("adds no listeners when the page gives no preview function", async () => {
    const link = mount(undefined);
    hover(link);
    await settle();
    expect(panel()).toBeNull();
    expect(link.hasAttribute("aria-describedby")).toBe(false);
  });
});
