#!/usr/bin/env node
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)));
const md = readFileSync(resolve(root, "listing.md"), "utf8");
const NAME_WORDS = ["文字化け", "しない", "ZIP"];

function field(id) {
  const match = md.match(new RegExp(`<!-- listing:${id} -->([\\s\\S]*?)<!-- /listing:${id} -->`));
  if (!match) {
    throw new Error(`listing.md に listing:${id} がありません`);
  }
  return match[1].replace(/^\n|\n$/g, "").trim();
}

function pngSize(buf) {
  if (buf.length < 24 || buf.toString("ascii", 1, 4) !== "PNG") {
    throw new Error("PNG ではありません");
  }
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

const errors = [];
function check(cond, message) {
  if (!cond) {
    errors.push(message);
  }
}

const name = field("name");
check(name.length > 0 && name.length <= 30, `アプリ名 ${name.length} 文字（≤30）: ${name}`);
check(name === "文字化けしないZIP", `アプリ名が「文字化けしないZIP」ではない: ${name}`);

const subtitles = field("subtitles")
  .split("\n")
  .map((line) => line.trim())
  .filter(Boolean);
check(subtitles.length >= 2, "サブタイトル候補が足りません");
for (const line of subtitles) {
  check(line.length <= 30, `サブタイトル ${line.length} 文字（≤30）: ${line}`);
}

const promo = field("promo");
check(promo.length > 0 && promo.length <= 170, `プロモーション ${promo.length} 文字（≤170）`);

const description = field("description");
check(
  description.length > 0 && description.length <= 4000,
  `説明 ${description.length} 文字（≤4000）`,
);
for (const lead of [
  "そのまま開ける",
  "追加",
  "Apple Silicon",
  ".DS_Store",
  "__MACOSX",
  "文字化け",
  "7-Zip",
]) {
  check(description.includes(lead), `説明の先頭要件に「${lead}」がありません`);
}

const keywords = field("keywords");
check(!/\s/.test(keywords), `キーワードに空白があります: ${keywords}`);
check(keywords.length <= 100, `キーワード ${keywords.length} 文字（≤100）`);
const tokens = keywords.split(",");
check(tokens.every((token) => token.length > 0), "空のキーワードがあります");
for (const word of NAME_WORDS) {
  check(
    !tokens.includes(word),
    `キーワードがアプリ名の語「${word}」を繰り返しています`,
  );
}

const support = field("support-url");
const privacy = field("privacy-url");
check(/^https:\/\//.test(support), `サポート URL が https ではありません: ${support}`);
check(/^https:\/\//.test(privacy), `プライバシー URL が https ではありません: ${privacy}`);

const skipShots = process.env.STORE_LISTING_SKIP_SHOTS === "1";
const shotDir = resolve(root, "screenshots");
const shots = skipShots ? [] : readdirSync(shotDir).filter((name) => name.endsWith(".png"));
if (!skipShots) {
  check(shots.length >= 4 && shots.length <= 5, `スクリーンショットは 4〜5 枚: ${shots.join(", ")}`);
  for (const name of shots) {
    const { width, height } = pngSize(readFileSync(resolve(shotDir, name)));
    check(width === 2880 && height === 1800, `${name} が ${width}x${height}（2880x1800 が必要）`);
  }
}

console.log(`name=${name.length} promo=${promo.length} description=${description.length} keywords=${keywords.length}`);
for (const line of subtitles) {
  console.log(`subtitle=${line.length} ${line}`);
}
console.log(`shots=${shots.join(",")}`);

if (errors.length > 0) {
  for (const error of errors) {
    console.error(error);
  }
  process.exit(1);
}
