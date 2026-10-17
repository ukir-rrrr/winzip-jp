#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appRoot, "../..");
const outDir = resolve(repoRoot, "docs/store/screenshots");
const framePath = resolve(repoRoot, "docs/store/frame.html");
const previewUrl = process.env.STORE_SHOT_URL ?? "http://127.0.0.1:4174";
const WIDTH = 2880;
const HEIGHT = 1800;

const SHOTS = [
  {
    file: "01-windows.png",
    scene: "store-create",
    headline: "Windowsでも文字化けしない",
    note: "",
    assert: async (page) => {
      const text = await page.locator("[data-testid='file-list']").innerText();
      if (!text.includes("見積書_2026年10月.xlsx")) {
        throw new Error("作成一覧に日本語の見積ファイルがありません");
      }
    },
  },
  {
    file: "02-drop.png",
    scene: "",
    headline: "ドロップするだけでZIP作成",
    note: "",
    assert: async (page) => {
      const title = await page.locator("[data-testid='dropzone-title']").innerText();
      if (title !== "ファイルをドロップして ZIP を作成") {
        throw new Error(`作成ドロップの見出しが違います: ${title}`);
      }
    },
  },
  {
    file: "03-extract.png",
    scene: "store-extract",
    headline: "文字化けZIPもきれいに解凍",
    note: "",
    assert: async (page) => {
      const text = await page.locator("[data-testid='file-list']").innerText();
      if (!text.includes("株式会社サンプル.pdf")) {
        throw new Error("解凍一覧に日本語名がありません");
      }
    },
  },
  {
    file: "04-junk.png",
    scene: "store-junk",
    headline: ".DS_Storeなど不要ファイルを自動除外",
    note: "除外した項目：.DS_Store、資料/.DS_Store、__MACOSX",
    assert: async (page) => {
      const text = await page.locator("[data-testid='file-list']").innerText();
      if (text.includes(".DS_Store") || text.includes("__MACOSX")) {
        throw new Error("除外したはずの項目が一覧に残っています");
      }
      if (!text.includes("報告書.docx")) {
        throw new Error("残すファイルが一覧にありません");
      }
    },
  },
  {
    file: "05-extract-drop.png",
    scene: "s5",
    headline: "文字化けしたZIPをドロップして解凍",
    note: "",
    assert: async (page) => {
      const title = await page.locator("[data-testid='dropzone-title']").innerText();
      if (title !== "文字化けした ZIP をドロップして解凍") {
        throw new Error(`解凍ドロップの見出しが違います: ${title}`);
      }
    },
  },
];

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

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
    const req = http.get(previewUrl, (res) => {
      res.resume();
      resolveStart(undefined);
    });
    req.on("error", () => {
      const dist = resolve(appRoot, "dist/index.html");
      if (!existsSync(dist)) {
        reject(new Error("フロントの dist がありません。VITE_E2E=1 で build してください。"));
        return;
      }
      const port = new URL(previewUrl).port || "4174";
      const child = spawn(
        "pnpm",
        ["exec", "vite", "preview", "--host", "127.0.0.1", "--port", port, "--strictPort"],
        { cwd: appRoot, stdio: "inherit" },
      );
      child.on("error", reject);
      waitForHttp(previewUrl)
        .then(() => resolveStart(child))
        .catch(reject);
    });
  });
}

async function launchBrowser() {
  const preferred = process.env.STORE_SHOT_CHANNEL;
  const guessChrome =
    process.platform === "darwin" ||
    existsSync("/usr/local/bin/google-chrome") ||
    existsSync("/usr/bin/google-chrome");
  const channel = preferred ?? (guessChrome ? "chrome" : undefined);
  const args = [
    "--hide-scrollbars",
    "--font-render-hinting=none",
    "--disable-lcd-text",
    "--disable-gpu",
  ];
  try {
    return await chromium.launch({
      channel,
      headless: true,
      args,
      env: { ...process.env, LANG: "ja_JP.UTF-8" },
    });
  } catch (error) {
    if (!channel) {
      throw error;
    }
    console.warn(`channel=${channel} で起動できませんでした。バンドルの Chromium を試します`);
    return await chromium.launch({ headless: true, args });
  }
}

function fillFrame(headline, note, dataUrl) {
  let html = readFileSync(framePath, "utf8");
  html = html.replace("{{HEADLINE}}", escapeHtml(headline));
  html = html.replace("{{NOTE}}", escapeHtml(note));
  html = html.replace("{{IMAGE}}", dataUrl);
  if (!note) {
    html = html.replace('id="note" class="note"', 'id="note" class="note" hidden');
  }
  return html;
}

function pngSize(buf) {
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

const preview = await startPreviewIfNeeded();
mkdirSync(outDir, { recursive: true });
const browser = await launchBrowser();
try {
  await waitForHttp(previewUrl);
  const capture = await browser.newPage({
    viewport: { width: 1320, height: 760 },
    deviceScaleFactor: 2,
    colorScheme: "light",
    locale: "ja-JP",
  });
  const compose = await browser.newPage({
    viewport: { width: WIDTH, height: HEIGHT },
    deviceScaleFactor: 1,
    colorScheme: "light",
    locale: "ja-JP",
  });

  for (const shot of SHOTS) {
    const path = shot.scene ? `${previewUrl}/?scene=${shot.scene}` : `${previewUrl}/`;
    await capture.goto(path, { waitUntil: "networkidle" });
    await capture.addStyleTag({
      content:
        'html, body, [data-ui] { font-family: "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Noto Sans CJK JP", "Noto Sans JP", sans-serif !important; }',
    });
    await capture.locator("[data-testid='app-shell']").waitFor({ state: "visible" });
    await capture.evaluate(() => document.fonts.ready);
    await shot.assert(capture);
    const uiPng = await capture.locator("[data-testid='app-shell']").screenshot({
      type: "png",
      animations: "disabled",
    });
    const dataUrl = `data:image/png;base64,${uiPng.toString("base64")}`;
    await compose.setContent(fillFrame(shot.headline, shot.note, dataUrl), {
      waitUntil: "load",
    });
    await compose.addStyleTag({
      content:
        'html, body, [data-ui] { font-family: "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Noto Sans CJK JP", "Noto Sans JP", sans-serif !important; }',
    });
    await compose.evaluate(() => document.fonts.ready);
    const framed = await compose.screenshot({
      type: "png",
      clip: { x: 0, y: 0, width: WIDTH, height: HEIGHT },
      animations: "disabled",
    });
    const dest = resolve(outDir, shot.file);
    writeFileSync(dest, framed);
    const { width, height } = pngSize(framed);
    if (width !== WIDTH || height !== HEIGHT) {
      throw new Error(`${shot.file} が ${width}x${height}`);
    }
    console.log(`wrote ${dest}`);
  }
} finally {
  await browser.close();
  if (preview && typeof preview.kill === "function") {
    preview.kill("SIGTERM");
  }
}
