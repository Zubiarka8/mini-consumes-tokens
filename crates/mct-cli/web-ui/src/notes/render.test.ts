import { describe, expect, it, vi } from "vitest";
import { renderNote } from "./index";

const html = (source: string, ctx = {}) => renderNote(source, ctx).innerHTML;
const root = (source: string, ctx = {}) => renderNote(source, ctx);

describe("CommonMark and GFM", () => {
  it("nests lists instead of flattening them", () => {
    const body = root("- a\n  - b\n    - c\n- d");
    expect(body.querySelectorAll("ul ul ul")).toHaveLength(1);
    expect(body.querySelector("ul > li > ul > li")?.textContent).toContain("b");
  });

  it("renders task lists as disabled checkboxes", () => {
    const boxes = root(
      "- [ ] todo\n- [x] done",
    ).querySelectorAll<HTMLInputElement>("li input[type=checkbox]");
    expect([...boxes].map((box) => box.checked)).toEqual([false, true]);
    expect([...boxes].every((box) => box.disabled)).toBe(true);
  });

  it("does not repeat the task marker as text or draw two boxes", () => {
    const body = root("- [x] Done *em*\n- [ ] Open\n\n- [x] loose\n\n  more");
    expect(body.querySelectorAll("input[type=checkbox]")).toHaveLength(3);
    expect(body.textContent).not.toMatch(/\[[ x]\]/);
    expect(body.querySelector("li")?.textContent).toBe("Done em");
  });

  it("keeps an ordered list's start number", () => {
    expect(
      root("3. three\n4. four").querySelector("ol")?.getAttribute("start"),
    ).toBe("3");
  });

  it("handles strikethrough, setext headings, rules and hard breaks", () => {
    expect(html("~~gone~~")).toContain("<del>gone</del>");
    expect(root("Title\n=====").querySelector("h1")?.textContent).toBe("Title");
    expect(root("***\n\ntext\n\n___").querySelectorAll("hr")).toHaveLength(2);
    expect(html("one  \ntwo")).toContain("<br>");
  });

  it("aligns table columns", () => {
    const cells = root(
      "| a | b |\n|:--|--:|\n| 1 | 2 |",
    ).querySelectorAll<HTMLElement>("td");
    expect([...cells].map((cell) => cell.className)).toEqual([
      "align-left",
      "align-right",
    ]);
  });

  it("nests blockquotes", () => {
    expect(
      root("> a\n>> b").querySelectorAll("blockquote blockquote"),
    ).toHaveLength(1);
  });
});

describe("Obsidian syntax", () => {
  it("opens a wikilink by note name and shows the alias", () => {
    const onNote = vi.fn();
    const link = root("[[Note#Section|shown]]", {
      onNote,
    }).querySelector<HTMLAnchorElement>("a.wikilink");
    expect(link?.textContent).toBe("shown");
    link?.click();
    expect(onNote).toHaveBeenCalledWith("Note", true);
  });

  it("labels a section link with both parts", () => {
    expect(
      root("[[Note#Section]]").querySelector("a.wikilink")?.textContent,
    ).toBe("Note > Section");
  });

  it("does not open anything for a link inside the same note", () => {
    const body = root("[[#Heading]]");
    expect(body.querySelector("a")).toBeNull();
    expect(body.querySelector(".note-self-link")?.textContent).toBe("Heading");
  });

  it("turns a note embed into a link and an image embed into a placeholder", () => {
    const onNote = vi.fn();
    const body = root("![[Other note]] and ![[pic.png]]", { onNote });
    body.querySelector<HTMLAnchorElement>("a.note-embed")?.click();
    expect(onNote).toHaveBeenCalledWith("Other note", true);
    expect(
      body.querySelector(".note-embed-missing")?.getAttribute("title"),
    ).toBe("pic.png");
    expect(body.textContent).not.toMatch(/^!/);
  });

  it("renders tags as buttons but not numbers or mid-word hashes", () => {
    const onTag = vi.fn();
    const body = root("#project/x and #todo, issue #123 and a#b", { onTag });
    const tags = [...body.querySelectorAll<HTMLButtonElement>("button.tag")];
    expect(tags.map((tag) => tag.textContent)).toEqual(["#project/x", "#todo"]);
    tags[0]?.click();
    expect(onTag).toHaveBeenCalledWith("project/x");
  });

  it("renders callouts, foldable ones as details", () => {
    const plain = root("> [!warning] Careful\n> body text").querySelector(
      ".callout",
    );
    expect(plain?.getAttribute("data-callout")).toBe("warning");
    expect(plain?.querySelector(".callout-title")?.textContent).toBe("Careful");
    expect(plain?.querySelector(".callout-body")?.textContent).toContain(
      "body text",
    );

    const folded = root("> [!note]- Folded\n> hidden").querySelector(
      "details.callout",
    );
    expect(folded?.hasAttribute("open")).toBe(false);
    expect(
      root("> [!note]+ Open\n> shown")
        .querySelector("details.callout")
        ?.hasAttribute("open"),
    ).toBe(true);
    expect(
      root("> [!tip]\n> body").querySelector(".callout-title")?.textContent,
    ).toBe("Tip");
  });

  it("keeps inline formatting in a callout title and renders the rest of the quote in its body", () => {
    const callout = root(
      "> [!note] A **bold** title\n> first line\n>\n> - item\n> - other",
    ).querySelector(".callout");
    expect(callout?.querySelector(".callout-title strong")?.textContent).toBe(
      "bold",
    );
    expect(callout?.querySelector(".callout-title")?.textContent).toBe(
      "A bold title",
    );
    expect(callout?.querySelector(".callout-body p")?.textContent).toBe(
      "first line",
    );
    expect(callout?.querySelectorAll(".callout-body li")).toHaveLength(2);
  });

  it("renders a callout that has a title and no body", () => {
    const callout = root("> [!info] Only a title").querySelector(".callout");
    expect(callout?.querySelector(".callout-title")?.textContent).toBe(
      "Only a title",
    );
    expect(callout?.querySelector(".callout-body")?.children).toHaveLength(0);
  });

  it("keeps footnotes working next to callouts", () => {
    const note =
      "> [!note] Title\n> body with a [[Other note]] link\n\nText[^1].\n\n[^1]: the note";
    const body = root(note);
    expect(body.querySelector(".callout-body a.wikilink")?.textContent).toBe(
      "Other note",
    );
    expect(body.querySelectorAll("section.footnotes li")).toHaveLength(1);
  });

  it("drops %%comments%% but keeps them inside code fences", () => {
    expect(root("visible %%hidden%% after").textContent).toBe("visible  after");
    expect(root("a\n\n%%\nmulti\n\nline\n%%\n\nb").textContent).toBe("ab");
    expect(root("```\n%%kept%%\n```").textContent).toContain("%%kept%%");
  });

  it("highlights ==text== and shows math as code", () => {
    expect(html("==marked==")).toContain("<mark>marked</mark>");
    expect(
      root("$e^{i\\pi}+1=0$").querySelector("code.math")?.textContent,
    ).toBe("e^{i\\pi}+1=0");
    expect(root("it costs $5 and $6").querySelector("code.math")).toBeNull();
  });

  it("renders footnotes with a way back", () => {
    const body = root("text[^1]\n\n[^1]: the note");
    expect(body.querySelector("sup.footnote-ref a")?.getAttribute("href")).toBe(
      "#footnote-1",
    );
    expect(
      body.querySelector("section.footnotes li#footnote-1")?.textContent,
    ).toContain("the note");
    expect(body.querySelector("a.footnote-back")?.getAttribute("href")).toBe(
      "#footnote-ref-1",
    );
  });

  it("keeps footnote clicks away from the URL hash the dashboard routes on", () => {
    document.body.replaceChildren(root("text[^1]\n\n[^1]: the note"));
    const scroll = vi.fn();
    document.getElementById("footnote-1")!.scrollIntoView = scroll;
    const before = window.location.hash;
    document.querySelector<HTMLAnchorElement>("sup.footnote-ref a")!.click();
    expect(window.location.hash).toBe(before);
    expect(scroll).toHaveBeenCalledOnce();
    document.body.replaceChildren();
  });

  it("keeps no footnote state between notes", () => {
    const note = "text[^1]\n\n[^1]: the note";
    expect(root(note).querySelectorAll("section.footnotes li")).toHaveLength(1);
    expect(root("plain text").querySelector("section.footnotes")).toBeNull();
    expect(root(note).querySelectorAll("section.footnotes li")).toHaveLength(1);
  });
});

describe("code blocks", () => {
  it("highlights known languages with DOM nodes", () => {
    const code = root("```rust\nfn main() {}\n```").querySelector("pre code");
    expect(code?.querySelector(".hljs-keyword")?.textContent).toBe("fn");
    expect(
      root("```ts\nconst a = 1;\n```").querySelector(".hljs-keyword"),
    ).not.toBeNull();
  });

  it("shows unknown languages and tilde fences as plain text", () => {
    expect(
      root("```nope\nlet a;\n```").querySelector("pre code")?.textContent,
    ).toBe("let a;");
    expect(
      root("~~~js\nlet a = 1;\n~~~").querySelector("pre code")?.textContent,
    ).toBe("let a = 1;");
  });

  it("escapes markup inside a highlighted block", () => {
    const code = root("```html\n<script>alert(1)</script>\n```").querySelector(
      "pre code",
    );
    expect(code?.querySelector("script")).toBeNull();
    expect(code?.textContent).toContain("<script>alert(1)</script>");
  });
});

describe("links, images and raw HTML", () => {
  it("opens external links in a new tab without leaking the opener", () => {
    const link = root("[site](https://example.com/a)").querySelector("a");
    expect(link?.getAttribute("target")).toBe("_blank");
    expect(link?.getAttribute("rel")).toBe("noopener noreferrer nofollow");
  });

  it("opens a relative .md link as a note", () => {
    const onNote = vi.fn();
    root("[doc](docs/My%20note.md#top)", { onNote })
      .querySelector("a")
      ?.click();
    // The raw href: the page decodes it when it resolves the path.
    expect(onNote).toHaveBeenCalledWith("docs/My%20note.md#top", false);
  });

  it("never links an unsafe scheme, including with parentheses in the URL", () => {
    for (const source of [
      "[x](javascript:alert(1))",
      "[x](JaVaScRiPt:alert(1))",
      "[x](data:text/html,hi)",
      "[x](vbscript:x)",
    ]) {
      const body = root(source);
      expect(body.querySelector("a"), source).toBeNull();
      expect(body.querySelector("[href]"), source).toBeNull();
      expect(body.textContent, source).toContain("x");
    }
  });

  it("shows raw HTML as text", () => {
    const body = root(
      "<script>alert(1)</script> <b>x</b>\n\n<img src=x onerror=alert(1)>",
    );
    expect(body.querySelector("script, b, img")).toBeNull();
    expect(body.textContent).toContain("<script>alert(1)</script>");
    expect(body.textContent).toContain("<img src=x onerror=alert(1)>");
  });

  it("hides HTML comments", () => {
    expect(root("<!-- note to self -->\n\ntext").textContent).toBe("text");
  });

  it("renders only embedded raster images and a placeholder for anything else", () => {
    const pixel = "data:image/png;base64,iVBORw0KGgo=";
    expect(
      root(`![dot](${pixel})`).querySelector("img")?.getAttribute("src"),
    ).toBe(pixel);
    for (const src of [
      "pic.png",
      "https://example.com/x.png",
      "data:image/svg+xml;base64,PHN2Zz4=",
      "javascript:alert(1)",
    ]) {
      const body = root(`![alt](${src})`);
      expect(body.querySelector("img"), src).toBeNull();
      expect(body.querySelector(".note-image-missing")?.textContent, src).toBe(
        "alt",
      );
    }
  });

  it("never sets an inline style attribute (the dashboard's CSP forbids it)", () => {
    const body = root(
      "| a | b |\n|:--|--:|\n| 1 | 2 |\n\n```js\nlet a = 1;\n```\n\n> [!note]\n> x",
    );
    expect(body.querySelectorAll("[style]").length).toBe(0);
  });
});
