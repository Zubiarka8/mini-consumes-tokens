import { readdirSync, readFileSync } from "node:fs";
import { test, expect } from "@playwright/test";
const home = "/mini-consumes-tokens/";
test("English default, complete Spanish switch, persistence and English docs", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(home);
  await expect(page.locator("h1")).toContainText("Your AI reads less.");
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
  await page.reload();
  await expect(page.locator("h1")).toContainText("Tu IA lee menos.");
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
  await page.goto(home);
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
  await page.goto(home);
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
  await page.goto(home);
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
for (const width of [320, 375, 768, 1024, 1440])
  test(`no overflow in either language at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(home);
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
  await page.goto(home);
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
  await page.goto(home);
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
  await page.goto(home);
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
