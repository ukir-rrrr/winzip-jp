#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import http from "node:http";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const artifacts = resolve(appRoot, "e2e/artifacts");
const url = process.env.E2E_BASE_URL ?? "http://127.0.0.1:4173";
mkdirSync(artifacts, { recursive: true });

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

function startPreviewIfNeeded() {
  return new Promise((resolveStart, reject) => {
    const req = http.get(url, (res) => {
      res.resume();
      resolveStart(undefined);
    });
    req.on("error", () => {
      const dist = resolve(appRoot, "dist/index.html");
      if (!existsSync(dist)) {
        reject(new Error("フロントの dist がありません。VITE_E2E=1 で build してください。"));
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

function assertNoFakeProgress() {
  const srcRoot = resolve(appRoot, "src");
  const stack = [srcRoot];
  while (stack.length) {
    const dir = stack.pop();
    for (const name of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, name.name);
      if (name.isDirectory()) {
        if (name.name === "e2e") {
          continue;
        }
        stack.push(path);
        continue;
      }
      if (!/\.(ts|tsx|css)$/.test(name.name)) {
        continue;
      }
      const text = readFileSync(path, "utf8");
      if (text.includes("s4=1") || text.includes("isS4Draft")) {
        throw new Error(`${path} に本番用の ?s4=1 仮プログレスが残っています`);
      }
    }
  }
}

assertNoFakeProgress();

const preview = await startPreviewIfNeeded();
try {
  await waitForHttp(url);
  const wdio = spawnSync(
    "pnpm",
    ["exec", "wdio", "run", "e2e/wdio.conf.ts"],
    { cwd: appRoot, stdio: "inherit", env: { ...process.env, E2E_BASE_URL: url } },
  );
  if (wdio.status !== 0) {
    throw new Error(`WebdriverIO が失敗しました (exit ${wdio.status})`);
  }
  const compare = spawnSync("node", ["e2e/compare.mjs"], { cwd: appRoot, stdio: "inherit" });
  if (compare.status !== 0) {
    throw new Error("pixelmatch が失敗しました");
  }
} finally {
  if (preview && typeof preview.kill === "function") {
    preview.kill("SIGTERM");
  }
}
