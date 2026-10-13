import { test, expect } from "@playwright/test";

const home = "/mini-consumes-tokens/";
const languages = [
  {
    code: "en",
    layers: [
      "01 Your files",
      "02 Map of the pieces",
      "03 What the assistant needs",
    ],
    flow: [
      "Your files",
      "A map of the pieces and how they connect",
      "Only what the assistant needs",
    ],
    fullScreen: "Full screen",
    exit: "Exit full screen",
    dialog: "Code atlas",
    removed: [
      "MCT / 001",
      "Layer spacing",
      "THE AGENT’S QUESTION",
      "Focused context pack",
      "Symbol graph",
      "Symbols and resolved calls",
    ],
  },
  {
    code: "es",
    layers: [
      "01 Tus archivos",
      "02 Mapa de las piezas",
      "03 Lo que necesita el asistente",
    ],
    flow: [
      "Tus archivos",
      "Un mapa de las piezas y cómo se conectan",
      "Solo lo que necesita el asistente",
    ],
    fullScreen: "Pantalla completa",
    exit: "Salir de pantalla completa",
    dialog: "Atlas de código",
    removed: [
      "MCT / 001",
      "Separación de capas",
      "LA PREGUNTA DEL AGENTE",
      "Paquete de contexto preciso",
      "Grafo de símbolos",
      "Símbolos y llamadas resueltas",
    ],
  },
] as const;

for (const language of languages) {
  test(`atlas drops the symbol panel and speaks plainly in ${language.code}`, async ({
    page,
  }) => {
    await page.goto(home);
    if (language.code === "es")
      await page.getByLabel("Language", { exact: true }).selectOption("es");
    const atlas = page.locator(".atlas");
    await atlas.scrollIntoViewIfNeeded();
    for (const selector of [
      ".atlas-top",
      ".symbol-controls",
      ".context-pack",
      ".atlas-query",
      ".atlas-source",
      ".atlas-caption",
      ".atlas-separation",
    ])
      await expect(atlas.locator(selector)).toHaveCount(0);
    await expect(atlas.getByRole("slider")).toHaveCount(0);
    const text = await atlas.innerText();
    for (const removed of language.removed) expect(text).not.toContain(removed);
    expect(text).not.toMatch(/reindex|MctServer|ServerOptions|\.rs\b/);
    const toggles = atlas.locator(".atlas-layer-toggles button");
    await expect(toggles).toHaveCount(3);
    for (const [i, name] of language.layers.entries())
      await expect(toggles.nth(i)).toHaveAccessibleName(name);
    await expect(atlas.locator(".atlas-project-flow > span")).toHaveText([
      ...language.flow,
    ]);
    // The flow arrows are proportional icons, not text glyphs.
    await expect(atlas.locator(".atlas-project-flow svg")).toHaveCount(2);
    await expect(
      atlas.locator(".atlas-project-flow svg").first(),
    ).toHaveAttribute("aria-hidden", "true");
  });

  test(`atlas full screen, focus and Escape still work in ${language.code}`, async ({
    page,
  }) => {
    await page.goto(home);
    if (language.code === "es")
      await page.getByLabel("Language", { exact: true }).selectOption("es");
    const atlas = page.locator(".atlas");
    await atlas.scrollIntoViewIfNeeded();
    const open = atlas.getByRole("button", {
      name: language.fullScreen,
      exact: true,
    });
    await open.click();
    await expect(
      page.getByRole("dialog", { name: language.dialog, exact: true }),
    ).toBeVisible();
    await expect(
      atlas.getByRole("button", { name: language.exit, exact: true }),
    ).toBeFocused();
    await expect(atlas.locator(".atlas-stage").first()).toBeVisible();
    await page.screenshot({
      path: `test-results/plain-atlas-full-${language.code}.png`,
    });
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(open).toBeFocused();
  });
}

test("3D atlas labels are plain words and the spacing is fixed", async ({
  page,
}) => {
  await page.goto(home);
  const supported = await page.evaluate(
    () => !!document.createElement("canvas").getContext("webgl2"),
  );
  test.skip(!supported, "WebKit runtime has no WebGL2");
  const atlas = page.locator(".atlas");
  await atlas.locator(".atlas-space").scrollIntoViewIfNeeded();
  await expect(atlas).toHaveAttribute("data-scene", "ready");
  const labels = await atlas
    .locator(".spatial-labels button")
    .allTextContents();
  expect(labels.length).toBeGreaterThan(0);
  for (const label of labels) expect(label).not.toMatch(/_|::|reindex|Mct/);
  await expect(atlas.locator(".spatial-board-labels")).toContainText(
    "02 / Map of the pieces",
  );
});

for (const language of languages)
  for (const width of [1440, 768, 375])
    test(`atlas fits without horizontal overflow at ${width}px in ${language.code}`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(home);
      if (language.code === "es")
        await page.getByLabel("Language", { exact: true }).selectOption("es");
      const atlas = page.locator(".atlas");
      await atlas.scrollIntoViewIfNeeded();
      await page.waitForTimeout(800);
      await atlas.screenshot({
        path: `test-results/plain-atlas-${width}-${language.code}.png`,
      });
      const overflow = await page.evaluate(
        () =>
          document.documentElement.scrollWidth -
          document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
      const clipped = await atlas.evaluate((element) => {
        const box = element.getBoundingClientRect();
        return [...element.querySelectorAll("button, p, strong")].filter(
          (child) => {
            const rect = child.getBoundingClientRect();
            return (
              rect.width > 0 &&
              !child.closest(".spatial-labels, .spatial-file-labels") &&
              (rect.right > box.right + 1 || rect.left < box.left - 1)
            );
          },
        ).length;
      });
      expect(clipped).toBe(0);
    });
