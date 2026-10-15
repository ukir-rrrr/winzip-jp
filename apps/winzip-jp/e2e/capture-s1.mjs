#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import http from "node:http";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = resolve(appRoot, "e2e/artifacts");
const outFile = resolve(outDir, process.env.S1_OUT_NAME ?? "s1.png");
const url = process.env.S1_URL ?? "http://127.0.0.1:4173/";
mkdirSync(outDir, { recursive: true });

function waitForHttp(target, timeoutMs = 30000) {
  const started = Date.now();
  return new Promise((resolveWait, reject) => {
    const tick = () => {
      const req = http.get(target, (res) => {
        res.resume();
        resolveWait(undefined);
      });
      req.on("error", () => {
        if (Date.now() - started > timeoutMs) {
          reject(new Error(`${target} が ${timeoutMs}ms 以内に応答しませんでした`));
          return;
        }
        setTimeout(tick, 250);
      });
    };
    tick();
  });
}

function findChrome() {
  const candidates = [
    process.env.CHROME_PATH,
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/local/bin/google-chrome",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    const found = spawnSync("which", [name], { encoding: "utf8" });
    const path = found.stdout?.trim();
    if (found.status === 0 && path) {
      return path;
    }
  }
  return undefined;
}

function startPreviewIfNeeded() {
  return new Promise((resolveStart, reject) => {
    const req = http.get(url, (res) => {
      res.resume();
      resolveStart(undefined);
    });
    req.on("error", () => {
      const dist = resolve(appRoot, "dist/index.html");
      if (!existsSync(dist)) {
        reject(
          new Error(
            "フロントの dist がありません。先に `pnpm --filter winzip-jp build` を実行してください。",
          ),
        );
        return;
      }
      const child = spawn(
        "pnpm",
        ["exec", "vite", "preview", "--host", "127.0.0.1", "--port", "4173", "--strictPort"],
        { cwd: appRoot, stdio: "inherit" },
      );
      child.on("error", reject);
      waitForHttp(url)
        .then(() => resolveStart(child))
        .catch(reject);
    });
  });
}

function screenshotWithChrome(chromePath) {
  const result = spawnSync(
    chromePath,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      "--window-size=800,600",
      `--screenshot=${outFile}`,
      url,
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) {
    throw new Error(`Chrome のスクリーンショットに失敗しました (exit ${result.status})`);
  }
}

async function screenshotWithPlaywright() {
  spawnSync("npx", ["--yes", "playwright", "install", "chromium"], {
    stdio: "inherit",
    env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: process.env.PLAYWRIGHT_BROWSERS_PATH },
  });
  const script = `
    const { chromium } = require('playwright');
    (async () => {
      const browser = await chromium.launch({ args: ['--no-sandbox'] });
      const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
      await page.goto(process.env.S1_URL, { waitUntil: 'networkidle' });
      await page.screenshot({ path: process.env.S1_OUT, type: 'png' });
      await browser.close();
    })().catch((err) => {
      console.error(err);
      process.exit(1);
    });
  `;
  const result = spawnSync("npx", ["--yes", "-p", "playwright", "node", "-e", script], {
    stdio: "inherit",
    env: { ...process.env, S1_URL: url, S1_OUT: outFile },
  });
  if (result.status !== 0) {
    throw new Error("Playwright のスクリーンショットに失敗しました");
  }
}

const preview = await startPreviewIfNeeded();
try {
  await waitForHttp(url);
  const chrome = findChrome();
  if (chrome) {
    screenshotWithChrome(chrome);
  } else {
    await screenshotWithPlaywright();
  }
  if (!existsSync(outFile)) {
    throw new Error(`スクリーンショットが書き出されませんでした: ${outFile}`);
  }
  console.log(`wrote ${outFile}`);
} finally {
  if (preview) {
    preview.kill("SIGTERM");
  }
}
