import { defineConfig } from "@playwright/test";
const port = process.env.PORT ?? "4173";
export default defineConfig({
  testDir: "./tests",
  timeout: 30000,
  workers: 1,
  use: {
    browserName: "webkit",
    baseURL: `http://127.0.0.1:${port}`,
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run preview",
    url: `http://127.0.0.1:${port}/mini-consumes-tokens/`,
    reuseExistingServer: false,
  },
  reporter: "list",
});
