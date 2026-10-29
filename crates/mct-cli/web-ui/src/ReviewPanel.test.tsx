import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { ReviewPanel } from "./ReviewPanel";
import { drafts } from "./drafts";
import { bridge, deferred, duplicate, dead, failure } from "./fixtures";
import * as notify from "./notify";
import type { Review } from "./types";

vi.mock("./notify", () => ({ ok: vi.fn(), err: vi.fn() }));

const decision = () => screen.getByRole("combobox", { name: "review.stateLabel" });
const note = () => screen.getByRole("textbox", { name: "review.addNote" });
// "review.save", or "review.saving" while the request is pending.
const saveButton = () => screen.getByRole("button", { name: /^review\.sav(e|ing)$/ });

beforeEach(() => {
  drafts.clear();
  vi.mocked(notify.ok).mockClear();
  vi.mocked(notify.err).mockClear();
});

describe("ReviewPanel", () => {
  test("Save stays disabled until a decision is chosen", async () => {
    const user = userEvent.setup();
    render(<ReviewPanel item={duplicate()} bridge={bridge()} onSaved={vi.fn()} />);
    expect(saveButton()).toBeDisabled();
    await user.selectOptions(decision(), "confirmed");
    expect(saveButton()).toBeEnabled();
  });

  test("nothing is reported as saved before the server answers", async () => {
    const user = userEvent.setup();
    const answer = deferred<Review>();
    const save = vi.fn(() => answer.promise);
    const onSaved = vi.fn();
    render(<ReviewPanel item={duplicate()} bridge={bridge({ save })} onSaved={onSaved} />);
    await user.selectOptions(decision(), "confirmed");
    await user.type(note(), "same helper");
    await user.click(saveButton());
    expect(save).toHaveBeenCalledWith("k1", "confirmed", "same helper", "fp|r1");
    expect(saveButton()).toBeDisabled();
    expect(saveButton()).toHaveAttribute("aria-busy", "true");
    expect(notify.ok).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
    const saved = { ...duplicate(), state: "confirmed", note: "same helper" };
    await act(async () => answer.resolve(saved));
    expect(onSaved).toHaveBeenCalledWith(saved);
    expect(notify.ok).toHaveBeenCalledTimes(1);
    expect(drafts.has("k1")).toBe(false);
  });

  test("a failed save keeps the decision and note, and Retry resends them", async () => {
    const user = userEvent.setup();
    const save = vi.fn().mockRejectedValueOnce(failure(503, "review file is read-only"));
    const onSaved = vi.fn();
    render(<ReviewPanel item={duplicate()} bridge={bridge({ save })} onSaved={onSaved} />);
    await user.selectOptions(decision(), "dismissed");
    await user.type(note(), "generated code");
    await user.click(saveButton());
    expect(screen.getByRole("alert")).toHaveTextContent("review file is read-only");
    expect(note()).toHaveValue("generated code");
    expect(decision()).toHaveValue("dismissed");
    expect(onSaved).not.toHaveBeenCalled();
    save.mockResolvedValueOnce({ ...duplicate(), state: "dismissed" });
    await user.click(screen.getByRole("button", { name: "common.retry" }));
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1]).toEqual(save.mock.calls[0]);
    expect(onSaved).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  test("a conflict shows the current review and keeps the typed note", async () => {
    const user = userEvent.setup();
    const current = { ...duplicate(), state: "confirmed", revision: "fp|r2" };
    const save = vi.fn().mockRejectedValue(failure(409, "changed meanwhile", current));
    const onSaved = vi.fn();
    const { rerender } = render(<ReviewPanel item={duplicate()} bridge={bridge({ save })} onSaved={onSaved} />);
    await user.selectOptions(decision(), "false-positive");
    await user.type(note(), "my reasoning");
    await user.click(saveButton());
    expect(onSaved).toHaveBeenCalledWith(current);
    expect(screen.getByRole("alert")).toHaveTextContent("review.conflict");
    rerender(<ReviewPanel item={current} bridge={bridge({ save })} onSaved={onSaved} />);
    expect(note()).toHaveValue("my reasoning");
  });

  test("an unsaved draft survives switching files and findings", async () => {
    const user = userEvent.setup();
    const first = render(<ReviewPanel item={duplicate()} bridge={bridge()} onSaved={vi.fn()} />);
    await user.selectOptions(decision(), "in-review");
    await user.type(note(), "half written");
    expect(screen.getByText("review.unsaved")).toBeInTheDocument();
    first.unmount();
    const other = render(<ReviewPanel item={dead()} bridge={bridge()} onSaved={vi.fn()} />);
    expect(note()).toHaveValue("");
    other.unmount();
    render(<ReviewPanel item={duplicate()} bridge={bridge()} onSaved={vi.fn()} />);
    expect(note()).toHaveValue("half written");
    expect(decision()).toHaveValue("in-review");
  });

  test("marking a false positive asks first; cancelling saves nothing", async () => {
    const user = userEvent.setup();
    const save = vi.fn();
    const confirm = vi.fn(async () => false);
    render(<ReviewPanel item={duplicate()} bridge={bridge({ save, confirm })} onSaved={vi.fn()} />);
    await user.selectOptions(decision(), "false-positive");
    await user.click(saveButton());
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(save).not.toHaveBeenCalled();
  });

  test("Ignore dismisses in one click with the typed note", async () => {
    const user = userEvent.setup();
    const save = vi.fn(async () => ({ ...dead(), state: "dismissed" }));
    const onSaved = vi.fn();
    render(<ReviewPanel item={dead()} bridge={bridge({ save })} onSaved={onSaved} />);
    await user.type(note(), "test fixture");
    await user.click(screen.getByRole("button", { name: /review\.ignoreNow/ }));
    expect(save).toHaveBeenCalledWith("d1", "dismissed", "test fixture", "fp|r1");
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  test("a read-only review store shows why and offers no form", () => {
    render(<ReviewPanel item={duplicate({ readOnly: true })} bridge={bridge()} onSaved={vi.fn()} />);
    expect(screen.getByText("review.readOnly")).toBeInTheDocument();
    expect(screen.queryByRole("combobox")).toBeNull();
  });
});
