import { readdirSync, readFileSync } from "node:fs";
import { test, expect, type Page } from "@playwright/test";
const home = "/mini-consumes-tokens/";
async function visitHome(page: Page) {
  await page.goto(home);
  await expect(page.locator(".landing-header")).toHaveAttribute(
    "data-menu-ready",
    "true",
  );
}
for (const width of [320, 375, 768, 1024, 1100])
  test(`mobile navigation opens and closes accessibly at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await visitHome(page);
    const toggle = page.getByRole("button", { name: /^(Open|Close) menu$/ });
    const navigation = page.getByRole("navigation", {
      name: "Main navigation",
    });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(navigation).toBeHidden();
    await expect(page.getByLabel("Language", { exact: true })).toBeHidden();
    await toggle.focus();
    await page.keyboard.press("Enter");
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(navigation).toBeVisible();
    await page.keyboard.press("Alt+Tab");
    await expect(
      navigation.getByRole("link", { name: "Why", exact: true }),
    ).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(toggle).toBeFocused();
    await toggle.click();
    await page.getByLabel("Language", { exact: true }).selectOption("es");
    await expect(
      page.getByRole("button", { name: "Cerrar menú" }),
    ).toBeVisible();
    await page.getByRole("link", { name: "Por qué", exact: true }).click();
    await expect(page).toHaveURL(/#problem$/);
    await expect(
      page.getByRole("button", { name: "Abrir menú" }),
    ).toHaveAttribute("aria-expanded", "false");
    await expect(
      page.getByRole("navigation", { name: "Navegación principal" }),
    ).toBeHidden();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    expect(errors).toEqual([]);
  });

for (const viewport of [
  { width: 375, height: 900 },
  { width: 844, height: 390 },
])
  test(`mobile menu remains reachable in both themes at ${viewport.width}x${viewport.height}`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await visitHome(page);
    await page.getByRole("button", { name: "Open menu", exact: true }).click();
    for (const theme of ["light", "dark"]) {
      await page.getByLabel("Color theme", { exact: true }).selectOption(theme);
      if (theme === "dark")
        await expect(page.locator("html")).toHaveClass(/dark/);
      else await expect(page.locator("html")).not.toHaveClass(/dark/);
      const docs = page
        .locator(".header-actions")
        .getByRole("link", { name: "Documentation" });
      await docs.scrollIntoViewIfNeeded();
      const bounds = await docs.boundingBox();
      if (!bounds) throw new Error("Documentation link has no rendered bounds");
      expect(bounds.y).toBeGreaterThanOrEqual(0);
      expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height);
      const controls = await page
        .locator(
          ".landing-header nav a, .header-actions select, .header-docs, .mobile-menu-toggle",
        )
        .evaluateAll((elements) =>
          elements.map((element) => {
            const { width, height } = element.getBoundingClientRect();
            return { width, height };
          }),
        );
      for (const control of controls) {
        expect(control.width).toBeGreaterThanOrEqual(44);
        expect(control.height).toBeGreaterThanOrEqual(44);
      }
      await page.screenshot({
        path: `test-results/mobile-menu-${viewport.width}-${theme}.png`,
      });
      await page
        .getByRole("button", { name: "Close menu", exact: true })
        .click();
      await page.screenshot({
        path: `test-results/mobile-menu-${viewport.width}-${theme}-closed.png`,
      });
      await page
        .getByRole("button", { name: "Open menu", exact: true })
        .click();
    }
  });

test("mobile menu closes outside and resets after desktop navigation", async ({
  page,
}) => {
  await page.setViewportSize({ width: 375, height: 900 });
  await visitHome(page);
  const toggle = page.locator(".mobile-menu-toggle");
  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  await toggle.click();
  await page.locator("h1").click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await toggle.click();
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(toggle).toBeHidden();
  await expect(navigation).toBeVisible();
  await expect(page.getByLabel("Language", { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 375, height: 900 });
  await expect(navigation).toBeHidden();
  await toggle.click();
  await page
    .locator(".header-actions")
    .getByRole("link", { name: "Documentation" })
    .click();
  await expect(page).toHaveURL(/\/docs\/$/);
});

test("mobile navigation remains available without JavaScript", async ({
  browser,
}) => {
  const context = await browser.newContext({
    javaScriptEnabled: false,
    viewport: { width: 320, height: 800 },
  });
  const page = await context.newPage();
  await page.goto(home);
  await expect(page.locator(".mobile-menu-toggle")).toBeHidden();
  await expect(
    page.getByRole("navigation", { name: "Main navigation" }),
  ).toBeVisible();
  await page
    .locator(".header-actions")
    .getByRole("link", { name: "Documentation" })
    .click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await context.close();
});

test("English default, complete Spanish switch, persistence and English docs", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await visitHome(page);
  await expect(page.locator("h1")).toContainText("Your AI reads less.");
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
  // WebKit reports cancelled static RSC prefetches as page errors on unload.
  await page.waitForLoadState("networkidle");
  await page.reload();
  await expect(page.locator(".landing-header")).toHaveAttribute(
    "data-menu-ready",
    "true",
  );
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
  await page.waitForLoadState("networkidle");
  await page.getByRole("link", { name: "Instálalo gratis" }).first().click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  expect(errors).toEqual([]);
  page.removeAllListeners("pageerror");
  await page.reload();
  await expect(page.locator("h1")).toBeVisible();
  await page.goto(home + "docs/installation/");
  await expect(page.locator("h1")).toContainText("Installation");
});
test("clipboard failure, FAQ and keyboard", async ({ page }) => {
  await visitHome(page);
  await page.evaluate(() =>
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("denied")) },
      configurable: true,
    }),
  );
  await page
    .getByRole("button", {
      name: "Copy command: mct-cli --root . init",
      exact: true,
    })
    .click();
  await expect(
    page.locator(".command-panel").getByRole("status").first(),
  ).toContainText("Copy unavailable");
  await page
    .locator("summary")
    .filter({ hasText: "Where is the map stored?" })
    .click();
  await expect(page.locator("details").first()).toHaveAttribute("open", "");
  await page.goto("about:blank");
  await visitHome(page);
});
test("storage denied still permits language changes", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(Storage.prototype, "getItem", {
      value: () => {
        throw new Error("denied");
      },
    });
    Object.defineProperty(Storage.prototype, "setItem", {
      value: () => {
        throw new Error("denied");
      },
    });
  });
  await visitHome(page);
  await expect(page.locator("h1")).toContainText("Your AI reads less.");
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
});
test("no JavaScript retains English content and direct docs links", async ({
  browser,
}) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto(home);
  await expect(page.locator("h1")).toContainText("Your AI reads less.");
  await page.getByRole("link", { name: "Install it free" }).first().click();
  await expect(page.locator("h1")).toBeVisible();
  await context.close();
});
for (const width of [320, 375, 768, 1024, 1101, 1280, 1440])
  test(`no overflow in either language at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await visitHome(page);
    if (width <= 1100) {
      await page
        .getByRole("button", { name: "Open menu", exact: true })
        .click();
    }
    for (const language of ["en", "es"]) {
      await page.getByLabel("Language", { exact: true }).selectOption(language);
      const brand = page.locator(".brand > span");
      await expect(brand).toHaveText("mini-consumes-tokens");
      expect(
        await brand.evaluate((element) => {
          const range = document.createRange();
          range.selectNodeContents(element);
          return range.getClientRects().length;
        }),
      ).toBe(1);
      const overflow = await page.evaluate(() => ({
        width: document.documentElement.scrollWidth,
        viewport: innerWidth,
        elements: [...document.querySelectorAll("main *")]
          .filter(
            (element) => element.getBoundingClientRect().right > innerWidth,
          )
          .map((element) => element.className.toString())
          .slice(0, 12)
          .join(", "),
      }));
      expect(overflow.width, overflow.elements).toBeLessThanOrEqual(
        overflow.viewport,
      );
      await page.screenshot({
        path: `test-results/landing-${width}-${language}.png`,
        fullPage: true,
      });
    }
  });
test("Spanish browser starts in Spanish, others in English, invalid storage is ignored", async ({
  browser,
}) => {
  const context = await browser.newContext({ locale: "es-ES" });
  const page = await context.newPage();
  await page.addInitScript(() =>
    localStorage.setItem("mct-home-language", "invalid"),
  );
  await visitHome(page);
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
  await context.close();
  const other = await browser.newContext({ locale: "fr-FR" });
  const french = await other.newPage();
  await french.goto(home);
  await expect(french.locator("h1")).toContainText("Your AI reads less.");
  await other.close();
});
test("copy success uses the command and translated feedback", async ({
  page,
}) => {
  await visitHome(page);
  await page.evaluate(() =>
    Object.defineProperty(navigator, "clipboard", {
      value: {
        writeText: async (value: string) => {
          (window as Window & { copiedCommand?: string }).copiedCommand = value;
        },
      },
      configurable: true,
    }),
  );
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await page
    .getByRole("button", {
      name: "Copiar comando: mct-cli --root . status",
      exact: true,
    })
    .click();
  await expect(
    page
      .locator(".command-row")
      .filter({ hasText: "status" })
      .first()
      .getByRole("status"),
  ).toContainText("Comando copiado");
  expect(
    await page.evaluate(
      () => (window as Window & { copiedCommand?: string }).copiedCommand,
    ),
  ).toBe("mct-cli --root . status");
});

test("hero selection keeps dependencies and original line numbers", async ({
  page,
}) => {
  await visitHome(page);
  const demo = page.locator(".context-demo");
  await expect(demo.locator(".demo-line")).toHaveCount(51);
  await expect(
    demo.getByRole("button", { name: "Full file", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await demo
    .getByRole("button", { name: "Relevant context", exact: true })
    .click();
  await expect(demo.locator(".demo-line")).toHaveCount(6);
  await expect(demo.locator(".demo-code")).toContainText(
    "const taxRate = 0.21;",
  );
  await expect(demo.locator(".demo-code")).toContainText("calculateTotal");
  await expect(demo.locator(".demo-code")).not.toContainText("formatPrice");
  expect(await demo.locator(".demo-line-number").allTextContents()).toEqual([
    "2",
    "5",
    "6",
    "7",
    "8",
    "9",
  ]);
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(
    demo.getByRole("button", { name: "Contexto relevante", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await demo
    .getByRole("button", { name: "Archivo completo", exact: true })
    .click();
  await expect(demo.locator(".demo-line")).toHaveCount(51);
  await expect(demo.locator(".demo-code")).toContainText("createInvoice");
  await page.screenshot({ path: "test-results/hero-full.png" });
  await demo
    .getByRole("button", { name: "Contexto relevante", exact: true })
    .click();
  await page.screenshot({ path: "test-results/hero-relevant.png" });
});
