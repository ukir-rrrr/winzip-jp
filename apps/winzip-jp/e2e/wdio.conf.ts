import { resolve } from "node:path";
import type { Options } from "@wdio/types";

const root = resolve(import.meta.dirname);
const appRoot = resolve(root, "..");

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./specs/**/*.ts"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      browserName: "chrome",
      "goog:chromeOptions": {
        args: [
          "--headless=new",
          "--disable-gpu",
          "--no-sandbox",
          "--hide-scrollbars",
          "--force-device-scale-factor=1",
          "--font-render-hinting=none",
          "--disable-lcd-text",
          "--window-size=800,600",
        ],
      },
    },
  ],
  logLevel: "warn",
  baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:4173",
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 2,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },
  outputDir: resolve(appRoot, "e2e/artifacts/wdio-logs"),
};
