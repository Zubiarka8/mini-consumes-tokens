import { expect, test, type Page } from "@playwright/test";

const home = "/mini-consumes-tokens/";
const sizes = [
  { width: 1440, height: 900 },
  { width: 768, height: 1024 },
  { width: 375, height: 812 },
];

async function setSpanish(page: Page) {
  await page.locator(".locale-control select").selectOption("es");
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
}

test("documentation language notice appears only in Spanish", async ({
  page,
}) => {
  await page.goto(home);
  const docs = page.locator("#docs-title").locator("..");
  await expect(docs).toBeVisible();
  await expect(docs).not.toContainText(/in English|en inglés/i);
  await setSpanish(page);
  await expect(docs).toContainText("en inglés");
  await page.locator(".locale-control select").selectOption("en");
  await expect(docs).not.toContainText(/in English|en inglés/i);
});

for (const lang of ["en", "es"]) {
  test(`conceptual comparison note is absent (${lang})`, async ({ page }) => {
    await page.goto(home);
    if (lang === "es") await setSpanish(page);
    const problem = page.locator(".problem");
    await expect(problem).toBeVisible();
    await expect(problem).not.toContainText(
      /conceptual comparison|token benchmark|comparación conceptual|medición de tokens/i,
    );
    await expect(problem.locator(".caption")).toHaveCount(0);
  });
  test(`FAQ uses chevrons and exposes aria-expanded (${lang})`, async ({
    page,
  }) => {
    await page.goto(home);
    if (lang === "es") await setSpanish(page);
    const summaries = page.locator(".faq summary");
    await expect(summaries).toHaveCount(4);
    for (let i = 0; i < 4; i++) {
      const summary = summaries.nth(i);
      await expect(summary).not.toContainText(/[+×]/);
      await expect(summary).toHaveAttribute("aria-expanded", "false");
      await expect(summary.locator("svg.lucide-chevron-down")).toHaveCount(1);
      await expect(summary.locator("svg.lucide-chevron-up")).toHaveCount(0);
      const box = await summary.boundingBox();
      expect(box!.height).toBeGreaterThanOrEqual(44);
    }
    await summaries.first().click();
    await expect(summaries.first()).toHaveAttribute("aria-expanded", "true");
    await expect(
      summaries.first().locator("svg.lucide-chevron-up"),
    ).toHaveCount(1);
    await expect(
      summaries.first().locator("svg.lucide-chevron-down"),
    ).toHaveCount(0);
    await summaries.first().click();
    await expect(summaries.first()).toHaveAttribute("aria-expanded", "false");
  });
}

for (const size of sizes) {
  for (const lang of ["en", "es"]) {
    test(`Tab never reaches a hidden element (${lang} ${size.width})`, async ({
      page,
    }) => {
      await page.setViewportSize(size);
      await page.goto(home);
      if (lang === "es") await setSpanish(page);
      await page.evaluate(() =>
        (document.activeElement as HTMLElement)?.blur(),
      );
      await page.evaluate(() => window.scrollTo(0, 0));
      const seen: string[] = [];
      const problems: string[] = [];
      for (let i = 0; i < 80; i++) {
        // WebKit on macOS uses Option+Tab to include links in traversal.
        await page.keyboard.press("Alt+Tab");
        const info = await page.evaluate(() => {
          const el = document.activeElement as HTMLElement | null;
          if (!el || el === document.body) return null;
          const r = el.getBoundingClientRect();
          const s = getComputedStyle(el);
          const o = getComputedStyle(el, null);
          return {
            label: `${el.tagName.toLowerCase()}.${el.className}|${(el.textContent || el.getAttribute("aria-label") || "").trim().slice(0, 40)}`,
            w: r.width,
            h: r.height,
            right: r.right,
            bottom: r.bottom,
            left: r.left,
            top: r.top,
            vw: innerWidth,
            vh: innerHeight,
            opacity: s.opacity,
            visibility: s.visibility,
            clip: s.clip,
            outlineStyle: o.outlineStyle,
            outlineWidth: o.outlineWidth,
            tag: el.tagName,
          };
        });
        if (!info) break;
        if (seen.includes(info.label)) break;
        seen.push(info.label);
        const inViewport =
          info.w > 1 &&
          info.h > 1 &&
          info.right > 0 &&
          info.bottom > 0 &&
          info.left < info.vw &&
          info.top < info.vh;
        if (!inViewport || info.opacity === "0" || info.visibility === "hidden")
          problems.push(`hidden focus: ${info.label} ${JSON.stringify(info)}`);
        if (info.outlineStyle === "none" || info.outlineWidth === "0px")
          problems.push(`no focus ring: ${info.label}`);
      }
      expect(seen.length).toBeGreaterThan(5);
      expect(problems).toEqual([]);
    });
  }
}
