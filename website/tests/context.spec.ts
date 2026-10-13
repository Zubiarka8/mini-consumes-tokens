import { test, expect } from "@playwright/test";

const home = "/mini-consumes-tokens/";
const sizes = [
  { width: 1440, height: 900 },
  { width: 768, height: 1024 },
  { width: 375, height: 800 },
];

test("demo count uses an icon arrow, not a text arrow", async ({ page }) => {
  await page.goto(home);
  const count = page.locator(".context-demo .demo-count");
  await expect(count).not.toContainText("→");
  const arrow = count.locator("svg[aria-hidden='true']");
  await expect(arrow).toHaveCount(1);
  const [arrowBox, numberBox] = await Promise.all([
    arrow.boundingBox(),
    count.locator("strong").first().boundingBox(),
  ]);
  expect(arrowBox!.height).toBeGreaterThanOrEqual(numberBox!.height * 0.6);
});

test("demo lists plain-language benefits with icons in EN and ES", async ({
  page,
}) => {
  await page.goto(home);
  const items = page.locator(".context-demo .demo-benefits li");
  await expect(items).toHaveCount(4);
  for (const item of await items.all()) {
    await expect(item.locator("svg[aria-hidden='true']")).toHaveCount(1);
  }
  await expect(items.first()).toContainText(
    "Finds what matters without reading everything",
  );
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(items.first()).toContainText("Encuentra lo importante");
});

test("demo compares with and without MCP using text labels", async ({
  page,
}) => {
  await page.goto(home);
  const mcp = page.locator(".context-demo .demo-mcp");
  await expect(mcp).toContainText("MCP is");
  await expect(mcp.getByText("Without MCP", { exact: true })).toBeVisible();
  await expect(mcp.getByText("With MCP", { exact: true })).toBeVisible();
  await expect(mcp.locator(".demo-mcp-card")).toHaveCount(2);
  await expect(
    mcp.locator(".demo-mcp-card svg[aria-hidden='true']"),
  ).toHaveCount(2);
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(mcp.getByText("Sin MCP", { exact: true })).toBeVisible();
  await expect(mcp.getByText("Con MCP", { exact: true })).toBeVisible();
});

for (const size of sizes) {
  for (const lang of ["en", "es"]) {
    test(`demo has no overflow at ${size.width} (${lang})`, async ({
      page,
    }) => {
      await page.setViewportSize(size);
      await page.goto(home);
      await page.getByLabel("Language", { exact: true }).selectOption(lang);
      const demo = page.locator(".context-demo");
      await demo.scrollIntoViewIfNeeded();
      const overflow = await demo.evaluate((el) => {
        const root = document.documentElement;
        const bad = [...el.querySelectorAll(".demo-benefits *, .demo-mcp *")]
          .filter((n) => n.scrollWidth > n.clientWidth + 1)
          .map((n) => n.className);
        return { page: root.scrollWidth > root.clientWidth, bad };
      });
      expect(overflow.page).toBe(false);
      expect(overflow.bad).toEqual([]);
    });
  }
}
