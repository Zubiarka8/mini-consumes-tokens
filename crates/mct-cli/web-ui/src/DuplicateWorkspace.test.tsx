import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { App } from "./App";
import { fileLetter } from "./DuplicateWorkspace";
import { drafts } from "./drafts";
import { bridge, duplicate } from "./fixtures";
import type { Bridge, DuplicateFinding } from "./types";

function show(item: DuplicateFinding = duplicate(), file = 0, overrides: Partial<Bridge> = {}) {
  const props = {
    item,
    file,
    context: 3,
    bridge: bridge(overrides),
    onBack: vi.fn(),
    onSelectFile: vi.fn(),
    onContext: vi.fn(),
    onSaved: vi.fn(),
  };
  render(<App {...props} />);
  return props;
}

// A source that never answers: the columns keep showing the fragments the finding carries.
const pending = () => ({ source: vi.fn(() => new Promise<never>(() => {})) });
const column = (path: string) => screen.getByRole("region", { name: new RegExp(`^${path}:`) });
const lines = (region: HTMLElement, attribute: string) =>
  [...region.querySelectorAll(`[${attribute}]`)].map((line) => Number(line.getAttribute("data-line")));

beforeEach(() => drafts.clear());

describe("duplicate workspace", () => {
  test("shows the readable number and every file of the duplicate", () => {
    show();
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("find.number.duplicate(n=7)");
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    expect(within(files).getAllByRole("listitem")).toHaveLength(3);
  });

  test("the selected file is marked in the file list and its column", () => {
    show(duplicate(), 1);
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    expect(within(files).getByRole("button", { current: true })).toHaveTextContent("b.rs");
    expect(within(column("src/b.rs")).getByRole("button", { pressed: true })).toBeInTheDocument();
    expect(within(column("src/a.rs")).getByRole("button", { pressed: false })).toBeInTheDocument();
  });

  test("the bottom bar names the finding, sums it up and explains the colours", () => {
    show();
    const bar = screen.getByRole("contentinfo", { name: "rw.statusBar" });
    expect(within(bar).getByRole("heading", { level: 2 })).toBeInTheDocument();
    expect(bar).toHaveTextContent("dup.meta(files=3,lines=3,tokens=12)");
    expect(bar).toHaveTextContent("rw.matchLegend");
    expect(bar).toHaveTextContent("rw.diffLegend");
  });

  test("choosing a file in the list or a column header asks the page to select it", async () => {
    const user = userEvent.setup();
    const props = show();
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    await user.click(within(files).getByRole("button", { name: /c\.rs/ }));
    expect(props.onSelectFile).toHaveBeenLastCalledWith(2);
    await user.click(within(column("src/b.rs")).getByRole("button", { pressed: false }));
    expect(props.onSelectFile).toHaveBeenLastCalledWith(1);
  });

  test("each file is shown among its context lines, with its own lines highlighted", async () => {
    const props = show();
    expect(props.bridge.source).toHaveBeenCalledWith({ path: "src/a.rs", start: 1, end: 8 }, "review-member-0");
    expect(props.bridge.source).toHaveBeenCalledWith({ path: "src/b.rs", start: 5, end: 17 }, "review-member-1");
    expect(props.bridge.source).toHaveBeenCalledWith({ path: "src/c.rs", start: 15, end: 27 }, "review-member-2");
    await waitFor(() => expect(column("src/b.rs").querySelector('[data-line="5"]')).not.toBeNull());
    expect(lines(column("src/b.rs"), "data-block")).toEqual([10, 11, 12]);
    expect(column("src/b.rs").querySelectorAll("[data-line]")).toHaveLength(13);
  });

  test("the detector's differences mark their own lines in every column, with text and the value boxed", () => {
    show(duplicate(), 0, pending());
    expect(lines(column("src/a.rs"), "data-marked")).toEqual([1, 2]);
    expect(lines(column("src/b.rs"), "data-marked")).toEqual([10, 11]);
    expect(lines(column("src/c.rs"), "data-marked")).toEqual([20, 21]);
    const line = column("src/b.rs").querySelector('[data-line="11"]')!;
    expect(line).toHaveAttribute("title", expect.stringContaining("2"));
    expect(line).toHaveTextContent("rw.diffMark");
    expect(line.querySelector("mark")).toHaveTextContent(/^2$/);
    // The marks' screen-reader labels are absolute: the scrolling code view must contain them.
    expect(line.closest("[role=region]")).toHaveClass("relative");
  });

  test("no line is marked when the detector found no difference", () => {
    show(duplicate({ evidence: { identifierPattern: "identical", distinct: 0, differences: [], warnings: [] } }), 0, pending());
    expect(lines(column("src/a.rs"), "data-marked")).toEqual([]);
    expect(screen.getByText("rw.noDifferences")).toBeInTheDocument();
  });

  test("a listed difference jumps every file to its own line", async () => {
    const user = userEvent.setup();
    show(duplicate(), 0, pending());
    await user.click(screen.getByRole("button", { name: "rw.differences(n=2)" }));
    await user.click(screen.getByRole("button", { name: "rw.goToLine(line=2)" }));
    expect(column("src/a.rs").querySelector('[data-line="2"]')).toHaveAttribute("data-flash");
    expect(column("src/b.rs").querySelector('[data-line="11"]')).toHaveAttribute("data-flash");
    expect(column("src/c.rs").querySelector('[data-line="21"]')).toHaveAttribute("data-flash");
    expect(column("src/b.rs").querySelector('[data-line="10"]')).not.toHaveAttribute("data-flash");
  });

  test("a file can leave the comparison and come back from the file list", async () => {
    const user = userEvent.setup();
    const props = show();
    await user.click(screen.getByRole("button", { name: "rw.hideFile(path=src/c.rs)" }));
    expect(screen.queryByRole("region", { name: /^src\/c\.rs:/ })).toBeNull();
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    // A closed file reads as switched off, in words as well as in colour; an open one as lit.
    const closed = within(files).getByRole("button", { name: /c\.rs/ });
    expect(closed).toHaveTextContent("rw.fileClosed");
    expect(closed).toHaveClass("opacity-60");
    const open = within(files).getByRole("button", { name: /b\.rs/ });
    expect(open).toHaveTextContent("rw.fileOpen");
    expect(open).toHaveClass("bg-success-soft");
    await user.click(closed);
    expect(column("src/c.rs")).toBeInTheDocument();
    expect(within(files).getByRole("button", { name: /c\.rs/ })).toHaveClass("bg-success-soft");
    expect(props.onSelectFile).toHaveBeenLastCalledWith(2);
  });

  test("files are lettered A, B … Z, AA, AB … like spreadsheet columns", () => {
    expect([0, 1, 25, 26, 27, 51, 52, 701, 702].map(fileLetter)).toEqual(["A", "B", "Z", "AA", "AB", "AZ", "BA", "ZZ", "AAA"]);
  });

  test("each file shows its letter in the list and the column, and the review names the whole group by letter", () => {
    show();
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    expect(within(files).getAllByRole("img").map((chip) => chip.textContent)).toEqual(["A", "B", "C"]);
    expect(within(column("src/b.rs")).getByRole("img", { name: "dup.fileLetter(letter=B)" })).toBeInTheDocument();
    const review = screen.getByRole("complementary", { name: "rw.review" });
    expect(within(review).getByText("dup.decisionTarget")).toBeInTheDocument();
    expect(within(review).getAllByRole("img").map((chip) => chip.textContent)).toEqual(["A", "B", "C"]);
  });

  test("the last file in the comparison cannot be removed", async () => {
    const user = userEvent.setup();
    show();
    await user.click(screen.getByRole("button", { name: "rw.hideFile(path=src/c.rs)" }));
    await user.click(screen.getByRole("button", { name: "rw.hideFile(path=src/b.rs)" }));
    expect(screen.queryByRole("button", { name: /^rw\.hideFile/ })).toBeNull();
  });

  test("the columns share one horizontally scrollable region", () => {
    show();
    const region = screen.getByRole("region", { name: "dup.compareTitle" });
    expect(region).toHaveClass("overflow-x-auto");
    expect(within(region).getAllByRole("region", { name: /^src\/.*:\d+–\d+$/ })).toHaveLength(3);
  });

  test("each column copies its own fragment, without the context, and opens its file", async () => {
    const user = userEvent.setup();
    const props = show();
    await waitFor(() => expect(column("src/b.rs").querySelector('[data-line="5"]')).not.toBeNull());
    await user.click(within(column("src/b.rs")).getByRole("button", { name: "ai.copyCode: src/b.rs" }));
    expect(await navigator.clipboard.readText()).toBe("src/b.rs:10-12\nline 10\nline 11\nline 12");
    await user.click(within(column("src/b.rs")).getByRole("button", { name: "code.openSource: src/b.rs" }));
    expect(props.bridge.openSource).toHaveBeenCalledWith({ path: "src/b.rs", line: 10, language: "rust" });
  });

  test("a file whose body did not fit the detail waits for the source API", async () => {
    const item = duplicate();
    item.members[2] = { ...item.members[2], code: null };
    show(item);
    // Numbers are highlighted in their own span, so the line's text is read whole.
    await waitFor(() => expect(column("src/c.rs").querySelector('[data-line="20"]')).toHaveTextContent("line 20"));
  });

  test("Back, at the foot of the file list, returns to the list", async () => {
    const user = userEvent.setup();
    const props = show();
    const files = screen.getByRole("navigation", { name: "dup.fileListLabel" });
    await user.click(within(files).getByRole("button", { name: "dup.backToList" }));
    expect(props.onBack).toHaveBeenCalledTimes(1);
  });
});
