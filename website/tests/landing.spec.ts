import { readdirSync, readFileSync } from "node:fs";
import { test, expect } from "@playwright/test";
const home = "/mini-consumes-tokens/";
test("English default, complete Spanish switch, persistence and English docs", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(home);
  await expect(page.locator("h1")).toContainText("Less searching.");
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(page.locator("h1")).toContainText("Menos búsquedas.");
  await expect(page.locator("html")).toHaveAttribute("lang", "es");
  await expect(page.getByRole("img", { name: /Los archivos/ })).toBeVisible();
  await page.reload();
  await expect(page.locator("h1")).toContainText("Menos búsquedas.");
  await page
    .getByRole("link", { name: "Explora la documentación" })
    .first()
    .click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  expect(errors).toEqual([]);
  page.removeAllListeners("pageerror");
  await page.reload();
  await expect(page.locator("h1")).toBeVisible();
  await page.goto(home + "docs/installation/");
  await expect(page.locator("h1")).toContainText("Installation");
});
test("symbol selection, clipboard failure, FAQ and keyboard", async ({
  page,
}) => {
  await page.goto(home);
  await page
    .getByRole("button", { name: "handle_request", exact: true })
    .click();
  await expect(page.locator(".context-pack")).toContainText("src/server.rs:42");
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
    .filter({ hasText: "Where does the index live?" })
    .click();
  await expect(page.locator("details").first()).toHaveAttribute("open", "");
  await page.goto("about:blank");
  await page.goto(home);
  // WebKit on macOS uses Option+Tab to include links in keyboard traversal.
  await page.keyboard.press(process.platform === "darwin" ? "Alt+Tab" : "Tab");
  await expect(
    page.getByText("Skip to content", { exact: true }),
  ).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("#main")).toBeFocused();
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
  await expect(page.locator("h1")).toContainText("Less searching.");
  await page.getByLabel("Language", { exact: true }).selectOption("es");
  await expect(page.locator("h1")).toContainText("Menos búsquedas.");
});
test("no JavaScript retains English content and direct docs links", async ({
  browser,
}) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto(home);
  await expect(page.locator("h1")).toContainText("Less searching.");
  await page
    .getByRole("link", { name: "Explore the documentation" })
    .first()
    .click();
  await expect(page.locator("h1")).toBeVisible();
  await context.close();
});
for (const width of [320, 375, 768, 1024, 1440])
  test(`no overflow in either language at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(home);
    for (const language of ["en", "es"]) {
      await page.locator("select").selectOption(language);
      const brand = page.locator(".brand > span");
      await expect(brand).toHaveText("mini-consumes-tokens");
      expect(
        await brand.evaluate((element) => {
          const range = document.createRange();
          range.selectNodeContents(element);
          return range.getClientRects().length;
        }),
      ).toBe(1);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      await page.screenshot({
        path: `test-results/landing-${width}-${language}.png`,
        fullPage: true,
      });
    }
  });
for (const reducedMotion of ["reduce", "no-preference"] as const)
  test(`atlas hydrates without warnings with ${reducedMotion}`, async ({
    page,
    request,
  }) => {
    const errors: string[] = [];
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
    page.on("pageerror", (error) => errors.push(error.message));
    const response = await request.get(home);
    expect(response.ok()).toBe(true);
    expect(await response.text()).toContain('class="atlas-mode"></div>');
    await page.setViewportSize({ width: 1024, height: 900 });
    await page.emulateMedia({ reducedMotion });
    await page.goto(home);
    const button = page.locator(".atlas-mode button");
    if (reducedMotion === "reduce") await expect(button).toBeDisabled();
    else await expect(button).toBeEnabled();
    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect(button).toBeDisabled();
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await expect(button).toBeEnabled();
    await page.setViewportSize({ width: 375, height: 900 });
    await expect(button).toBeDisabled();
    expect(errors).toEqual([]);
  });

test("reduced motion keeps SVG and does not request the 3D bundle", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto(home);
  await expect(page.getByRole("img")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Explore in 3D" }),
  ).toBeDisabled();
  await expect(page.locator("canvas")).toHaveCount(0);
});
test("WebGL absence gives a readable 2D fallback", async ({ page }) => {
  await page.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (
      this: HTMLCanvasElement,
      type: string,
      ...args: any[]
    ) {
      return type === "webgl2"
        ? null
        : original.call(this, type as any, ...args);
    } as typeof original;
  });
  await page.goto(home);
  await page.getByRole("button", { name: "Explore in 3D" }).click();
  await expect(page.locator(".atlas-fallback")).toContainText(
    "2D atlas is active",
  );
  await expect(page.getByRole("img")).toBeVisible();
});
test("3D scene loads on request, rotates and falls back on context loss", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const state = { count: 0, lastDraw: 0 };
    (window as Window & { atlasDraws?: typeof state }).atlasDraws = state;
    const original = WebGL2RenderingContext.prototype.drawElements;
    WebGL2RenderingContext.prototype.drawElements = function (...args) {
      state.count++;
      state.lastDraw = Date.now();
      return original.apply(this, args);
    };
  });
  const draws = () =>
    page.evaluate(
      () =>
        (window as Window & { atlasDraws?: { count: number } }).atlasDraws
          ?.count ?? 0,
    );
  const settled = () =>
    page.evaluate(
      () =>
        Date.now() -
        ((window as Window & { atlasDraws?: { lastDraw: number } }).atlasDraws
          ?.lastDraw ?? Date.now()),
    );
  const chunkDirectory = "out/_next/static/chunks";
  const spatialChunks = readdirSync(chunkDirectory).filter(
    (name) =>
      name.endsWith(".js") &&
      readFileSync(`${chunkDirectory}/${name}`, "utf8").includes(
        "WebGLRenderer",
      ),
  );
  const requested: string[] = [];
  page.on("request", (request) =>
    requested.push(request.url().split("/").pop() ?? ""),
  );
  await page.goto(home);
  await expect(
    page.getByRole("button", { name: "Explore in 3D" }),
  ).toBeEnabled();
  expect(requested.filter((name) => spatialChunks.includes(name))).toEqual([]);
  await expect(page.locator("canvas")).toHaveCount(0);
  const supported = await page.evaluate(() => {
    const gl = document.createElement("canvas").getContext("webgl2");
    return !!gl;
  });
  test.skip(
    !supported,
    "WebKit runtime has no WebGL2; fallback verified separately",
  );
  let finishLoading!: () => void;
  const loading = new Promise<void>((resolve) => {
    finishLoading = resolve;
  });
  await page.route(
    (url) => spatialChunks.includes(url.pathname.split("/").pop() ?? ""),
    async (route) => {
      await loading;
      await route.continue();
    },
  );
  await page.getByRole("button", { name: "Explore in 3D" }).click();
  await expect(
    page.locator(".spatial-layer").getByRole("status"),
  ).toContainText("Loading the spatial atlas");
  await expect(page.getByRole("img")).toBeVisible();
  finishLoading();
  await expect(page.locator("canvas")).toBeVisible();
  expect(requested.some((name) => spatialChunks.includes(name))).toBe(true);
  await expect.poll(draws).toBeGreaterThan(0);
  await expect.poll(settled).toBeGreaterThan(100);
  const labels = page.locator(".spatial-labels");
  await expect(labels.getByRole("button")).toHaveCount(3);
  const selectedLabel = labels.getByRole("button", {
    name: "parse_request",
    exact: true,
  });
  await expect(selectedLabel).toBeVisible();
  const initialPosition = await selectedLabel.boundingBox();
  await labels
    .getByRole("button", { name: "handle_request", exact: true })
    .click();
  await expect(page.locator(".context-pack")).toContainText("src/server.rs:42");
  await expect(
    labels.getByRole("button", { name: "handle_request", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  const canvasBox = await page.locator("canvas").boundingBox();
  expect(canvasBox).not.toBeNull();
  const startX = canvasBox!.x + canvasBox!.width * 0.7;
  const startY = canvasBox!.y + canvasBox!.height * 0.3;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX - 100, startY + 40, { steps: 12 });
  await page.mouse.up();
  await expect
    .poll(async () =>
      Math.abs((await selectedLabel.boundingBox())!.x - initialPosition!.x),
    )
    .toBeGreaterThan(10);
  const orbitedPosition = await selectedLabel.boundingBox();
  await page.mouse.wheel(0, -200);
  await expect
    .poll(async () =>
      Math.abs((await selectedLabel.boundingBox())!.x - orbitedPosition!.x),
    )
    .toBeGreaterThan(1);
  await page.getByRole("button", { name: "Reset view", exact: true }).click();
  await expect
    .poll(async () =>
      Math.abs((await selectedLabel.boundingBox())!.x - initialPosition!.x),
    )
    .toBeLessThan(2);
  const firstDraws = await draws();
  await page.getByRole("button", { name: "Rotate the atlas" }).click();
  await expect.poll(draws).toBeGreaterThan(firstDraws);
  await expect.poll(settled).toBeGreaterThan(100);
  await page
    .locator(".atlas")
    .screenshot({ path: "test-results/landing-3d.png" });
  await page.locator("footer").scrollIntoViewIfNeeded();
  await expect(page.locator("canvas")).not.toBeInViewport();
  await expect.poll(settled).toBeGreaterThan(100);
  const offscreenDraws = await draws();
  await page
    .getByRole("button", { name: "Rotate the atlas" })
    .evaluate((button: HTMLButtonElement) => button.click());
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  expect(await draws()).toBe(offscreenDraws);
  await page.locator("canvas").scrollIntoViewIfNeeded();
  await expect.poll(draws).toBeGreaterThan(offscreenDraws);
  await page
    .locator("canvas")
    .evaluate((canvas) =>
      canvas.dispatchEvent(new Event("webglcontextlost", { cancelable: true })),
    );
  await expect(page.locator(".atlas-fallback")).toContainText(
    "2D atlas is active",
  );
  await expect(page.locator("canvas")).toHaveCount(0);
  await expect(page.getByRole("img")).toBeVisible();
});
test("3D chunk failure restores the visible SVG", async ({ page }) => {
  await page.goto(home);
  const supported = await page.evaluate(
    () => !!document.createElement("canvas").getContext("webgl2"),
  );
  test.skip(
    !supported,
    "WebGL2 is unavailable; unsupported-context fallback is covered separately",
  );
  const chunks = readdirSync("out/_next/static/chunks").filter(
    (name) =>
      name.endsWith(".js") &&
      readFileSync(`out/_next/static/chunks/${name}`, "utf8").includes(
        "WebGLRenderer",
      ),
  );
  await page.route(
    (url) => chunks.includes(url.pathname.split("/").pop() ?? ""),
    (route) => route.fulfill({ status: 503, body: "Unavailable" }),
  );
  await page.getByRole("button", { name: "Explore in 3D" }).click();
  await expect(page.locator(".atlas-fallback")).toContainText(
    "2D atlas is active",
  );
  await expect(page.getByRole("img")).toBeVisible();
  await expect(page.locator(".spatial-layer")).toHaveCount(0);
  await expect(page.locator(".atlas-mode button")).toHaveAttribute(
    "aria-pressed",
    "false",
  );
});
test("Spanish browser starts in English and invalid storage is ignored", async ({
  browser,
}) => {
  const context = await browser.newContext({ locale: "es-ES" });
  const page = await context.newPage();
  await page.addInitScript(() =>
    localStorage.setItem("mct-home-language", "invalid"),
  );
  await page.goto(home);
  await expect(page.locator("h1")).toContainText("Less searching.");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await context.close();
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
  await expect(page.getByRole("status").last()).toContainText(
    "Comando copiado",
  );
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
