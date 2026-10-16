#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)));
const artifacts = resolve(root, "artifacts");
const baselines = resolve(root, "baselines");
const ids = ["s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s9c", "s9d", "s10"];

// ツールバーと余白を除いたコンテンツ領域。信号ボタンはここに入らない。
const CROP = { x: 22, y: 64, width: 756, height: 514 };
const LIMIT = 0.01;

function crop(png) {
  const out = new PNG({ width: CROP.width, height: CROP.height });
  for (let y = 0; y < CROP.height; y += 1) {
    for (let x = 0; x < CROP.width; x += 1) {
      const sx = x + CROP.x;
      const sy = y + CROP.y;
      if (sx >= png.width || sy >= png.height) {
        continue;
      }
      const si = (sy * png.width + sx) * 4;
      const di = (y * CROP.width + x) * 4;
      out.data[di] = png.data[si];
      out.data[di + 1] = png.data[si + 1];
      out.data[di + 2] = png.data[si + 2];
      out.data[di + 3] = png.data[si + 3];
    }
  }
  return out;
}

function loadPng(path) {
  return PNG.sync.read(readFileSync(path));
}

mkdirSync(artifacts, { recursive: true });
mkdirSync(baselines, { recursive: true });

const shouldCompare =
  process.env.CI === "true" || process.env.COMPARE === "1" || process.env.UPDATE_BASELINES === "1";
if (!shouldCompare) {
  console.log("pixelmatch は CI / COMPARE=1 のときだけ実行します（ローカル Linux はフォントが違います）");
  process.exit(0);
}

let failed = 0;
let missing = 0;
for (const id of ids) {
  const actualPath = resolve(artifacts, `${id}.png`);
  const baselinePath = resolve(baselines, `${id}.png`);
  if (!existsSync(actualPath)) {
    console.error(`missing artifact ${id}.png`);
    failed += 1;
    continue;
  }
  if (process.env.UPDATE_BASELINES === "1") {
    writeFileSync(baselinePath, readFileSync(actualPath));
    console.log(`wrote baseline ${id}.png`);
    continue;
  }
  if (!existsSync(baselinePath)) {
    console.error(`missing baseline ${id}.png — macos-15 の artifact を e2e/baselines/ に置いてください`);
    missing += 1;
    continue;
  }
  const actual = crop(loadPng(actualPath));
  const baseline = crop(loadPng(baselinePath));
  if (actual.width !== baseline.width || actual.height !== baseline.height) {
    console.error(`${id}: size mismatch ${actual.width}x${actual.height} vs ${baseline.width}x${baseline.height}`);
    failed += 1;
    continue;
  }
  const diff = new PNG({ width: actual.width, height: actual.height });
  const mismatched = pixelmatch(actual.data, baseline.data, diff.data, actual.width, actual.height, {
    threshold: 0.1,
  });
  const ratio = mismatched / (actual.width * actual.height);
  writeFileSync(resolve(artifacts, `diff-${id}.png`), PNG.sync.write(diff));
  const pct = (ratio * 100).toFixed(3);
  if (ratio > LIMIT) {
    console.error(`${id}: pixelmatch ${pct}% > 1% (${mismatched} px)`);
    failed += 1;
  } else {
    console.log(`${id}: pixelmatch ${pct}%`);
  }
}

if (missing > 0 || failed > 0) {
  process.exit(1);
}
