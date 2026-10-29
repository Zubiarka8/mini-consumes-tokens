import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { App } from "./App";
import { drafts } from "./drafts";
import { bridge, dead } from "./fixtures";
import type { Bridge, DeadFinding } from "./types";

function show(item: DeadFinding = dead(), context = 3, overrides: Partial<Bridge> = {}) {
  const props = {
    item,
    file: 0,
    context,
    bridge: bridge(overrides),
    onBack: vi.fn(),
    onSelectFile: vi.fn(),
    onContext: vi.fn(),
    onSaved: vi.fn(),
  };
  const view = render(<App {...props} />);
  return { ...props, rerender: (next: Partial<typeof props>) => view.rerender(<App {...props} {...next} />) };
}

const code = () => screen.getByRole("region", { name: /^compare\.code/ });
// Numbers are highlighted in their own span, so a line's text is read whole.
const loadedLine = async (line: number) => {
  const region = await screen.findByRole("region", { name: /^compare\.code/ });
  await waitFor(() => expect(region.querySelector(`[data-line="${line}"]`)).not.toBeNull());
  return region.querySelector(`[data-line="${line}"]`)!;
};

beforeEach(() => drafts.clear());

describe("dead-code workspace", () => {
  test("shows the number, symbol, kind, file and lines", () => {
    show();
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("find.number.dead-code(n=3)");
    expect(screen.getAllByText("unused_helper").length).toBeGreaterThan(0);
    expect(screen.getByText("src/lib.rs")).toBeInTheDocument();
    expect(screen.getByText(/dup\.lineRange\(from=10,to=12\)/)).toBeInTheDocument();
  });

  test("loads the symbol with its context lines and marks only the symbol", async () => {
    const props = show();
    expect(await loadedLine(7)).toHaveTextContent("line 7");
    expect(props.bridge.source).toHaveBeenCalledWith({ path: "src/lib.rs", start: 7, end: 15 }, "review-dead-source");
    const marked = [...code().querySelectorAll("[data-marked]")].map((line) => line.getAttribute("data-line"));
    expect(marked).toEqual(["10", "11", "12"]);
    expect(code().querySelector('[data-line="10"]')).toHaveTextContent("rw.symbolLine");
  });

  test("context never starts before line 1 and changing it asks the page", async () => {
    const user = userEvent.setup();
    const props = show(dead(), 25);
    expect(await loadedLine(1)).toHaveTextContent("line 1");
    expect(props.bridge.source).toHaveBeenCalledWith({ path: "src/lib.rs", start: 1, end: 37 }, "review-dead-source");
    await user.selectOptions(screen.getByRole("combobox", { name: "dead.context" }), "10");
    expect(props.onContext).toHaveBeenCalledWith(10);
  });

  test("a failed load can be retried", async () => {
    const user = userEvent.setup();
    const source = vi.fn().mockRejectedValueOnce(Object.assign(new Error("gone"), { status: 404 }));
    source.mockResolvedValueOnce({ start: 7, lines: ["line 7"], truncated: false });
    show(dead(), 3, { source });
    await user.click(await screen.findByRole("button", { name: "common.retry" }));
    expect(await loadedLine(7)).toHaveTextContent("line 7");
  });

  test("the zero-references caution is visible without opening anything", () => {
    show();
    const review = screen.getByRole("complementary", { name: "rw.review" });
    expect(within(review).getByRole("note")).toHaveTextContent("dead.caution");
  });

  test("references are open in the review panel, and the detection reason is one click away", async () => {
    const user = userEvent.setup();
    show();
    expect(screen.getByRole("button", { name: "dead.refsTitle", expanded: true })).toBeInTheDocument();
    expect(screen.getByText(/dead\.imports\(n=1,m=1\)/)).toBeInTheDocument();
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "dead.why" }));
    expect(screen.getByText("dead.whyText(name=unused_helper)")).toBeInTheDocument();
  });

  test("the candidate's lines are highlighted among their context", async () => {
    show();
    await loadedLine(7);
    const block = [...code().querySelectorAll('[data-marked]')].filter((line) => line.classList.contains("bg-match"));
    expect(block).toHaveLength(3);
    expect(code().querySelector('[data-line="7"]')).not.toHaveClass("bg-match");
  });

  test("the code column copies the code and the path, and opens the source", async () => {
    const user = userEvent.setup();
    const props = show();
    const editor = screen.getByRole("region", { name: "rw.code" });
    await user.click(within(editor).getByRole("button", { name: "code.copyPath" }));
    expect(await navigator.clipboard.readText()).toBe("src/lib.rs:10-12");
    await loadedLine(10);
    await user.click(within(editor).getByRole("button", { name: "ai.copyCode" }));
    expect(await navigator.clipboard.readText()).toBe("src/lib.rs:10-12\nline 10\nline 11\nline 12");
    await user.click(within(editor).getByRole("button", { name: "code.openSource" }));
    expect(props.bridge.openSource).toHaveBeenCalledWith({ path: "src/lib.rs", line: 10, language: "rust" });
  });

  test("the bottom bar names the finding and explains the highlight; Back sits under the file", async () => {
    const user = userEvent.setup();
    const props = show();
    const bar = screen.getByRole("contentinfo", { name: "rw.statusBar" });
    expect(within(bar).getByRole("heading", { level: 2 })).toHaveTextContent("find.number.dead-code(n=3)");
    expect(bar).toHaveTextContent("dead.unreferenced(n=3)");
    const file = screen.getByRole("navigation", { name: "rw.file" });
    expect(file).toHaveTextContent("lib.rs");
    await user.click(within(file).getByRole("button", { name: "dup.backToList" }));
    expect(props.onBack).toHaveBeenCalledTimes(1);
  });
});
