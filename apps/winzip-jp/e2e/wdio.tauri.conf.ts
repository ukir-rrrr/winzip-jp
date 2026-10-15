import { resolve } from "node:path";
import type { Options } from "@wdio/types";

const appBinaryPath =
  process.env.TAURI_APP_PATH ??
  resolve(import.meta.dirname, "../../src-tauri/target/debug/winzip-jp");

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./specs/**/*.ts"],
  maxInstances: 1,
  capabilities: [{ maxInstances: 1, browserName: "tauri" }],
  services: [
    [
      "tauri",
      {
        appBinaryPath,
        driverProvider: "embedded",
      },
    ],
  ],
  logLevel: "warn",
  waitforTimeout: 15000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 2,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 120000,
  },
};
