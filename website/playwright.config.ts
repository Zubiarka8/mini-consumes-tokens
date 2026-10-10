import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  timeout: 30000,
  workers: 1,
  use: {
    browserName: "webkit",
    baseURL: "http://127.0.0.1:4173",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run preview",
    url: "http://127.0.0.1:4173/mini-consumes-tokens/",
    reuseExistingServer: !process.env.CI,
  },
  reporter: "list",
});
