#!/usr/bin/env node
import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)));
const docsRoot = resolve(root, "..");
const md = readFileSync(resolve(root, "listing.md"), "utf8");
const ADOPTED_SUBTITLE = "日本語ファイル名が化けないZIP作成・解凍";
const SUPPORT_URL = "https://ukir-rrrr.github.io/winzip-jp/support/";
const PRIVACY_URL = "https://ukir-rrrr.github.io/winzip-jp/privacy/";
const RESERVED_WORDS = [
  "文字化け",
  "しない",
  "ZIP",
  "日本語",
  "ファイル名",
  "化けない",
  "作成",
  "解凍",
];
const EXPECTED_SHOTS = [
  "01-anypc.png",
  "02-drop.png",
  "03-extract.png",
  "04-junk.png",
  "05-extract-drop.png",
];
const THIRD_PARTY_NAME = /Windows|WinZip/;

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
check(subtitles.length === 1, `提出するサブタイトルは 1 本: ${subtitles.join(" / ")}`);
check(subtitles[0] === ADOPTED_SUBTITLE, `採用サブタイトルと違う: ${subtitles[0]}`);
check(subtitles[0].length <= 30, `サブタイトル ${subtitles[0].length} 文字（≤30）`);

const promo = field("promo");
check(promo.length > 0 && promo.length <= 170, `プロモーション ${promo.length} 文字（≤170）`);

const description = field("description");
check(
  description.length > 0 && description.length <= 4000,
  `説明 ${description.length} 文字（≤4000）`,
);

const listingFields = [
  ["name", name],
  ["subtitles", subtitles[0]],
  ["promo", promo],
  ["description", description],
];
for (const [id, value] of listingFields) {
  check(!THIRD_PARTY_NAME.test(value), `${id} に第三者名 Windows/WinZip があります（Guideline 4.1(a)）`);
}
const firstPara = description.split(/\n\n/)[0] ?? "";
let from = 0;
for (const lead of ["そのまま開ける", "追加", "Apple Silicon", ".DS_Store", "__MACOSX", "文字化け"]) {
  const at = firstPara.indexOf(lead, from);
  check(at >= 0, `説明の先頭がこの順で「${lead}」を含みません`);
  from = at + lead.length;
}
check(description.includes("7-Zip"), "説明に 7-Zip の注意がありません");

const keywords = field("keywords");
check(!/\s/.test(keywords), `キーワードに空白があります: ${keywords}`);
check(keywords.length <= 100, `キーワード ${keywords.length} 文字（≤100）`);
const tokens = keywords.split(",");
check(tokens.every((token) => token.length > 0), "空のキーワードがあります");
const reserved = new Set(RESERVED_WORDS.map((word) => word.toLowerCase()));
for (const token of tokens) {
  check(!reserved.has(token.toLowerCase()), `キーワードが名前またはサブタイトルの語「${token}」を繰り返しています`);
  check(!THIRD_PARTY_NAME.test(token), `キーワードに第三者名 Windows/WinZip があります: ${token}`);
}

const shotHeadlines = [...md.matchAll(/\| `screenshots\/[^`]+` \| ([^|]+) \|/g)].map((row) => row[1].trim());
check(shotHeadlines.includes("どのパソコンでも文字化けしない"), "スクショ 01 の見出しが「どのパソコンでも文字化けしない」ではありません");
for (const headline of shotHeadlines) {
  check(!THIRD_PARTY_NAME.test(headline), `スクショ見出しに第三者名 Windows/WinZip があります: ${headline}`);
}

const support = field("support-url");
const privacy = field("privacy-url");
check(support === SUPPORT_URL, `サポート URL が想定と違います: ${support}`);
check(privacy === PRIVACY_URL, `プライバシー URL が想定と違います: ${privacy}`);

const supportHtml = readFileSync(resolve(docsRoot, "support/index.html"), "utf8");
const privacyHtml = readFileSync(resolve(docsRoot, "privacy/index.html"), "utf8");
check(supportHtml.includes('lang="ja"'), "サポートページが日本語ではありません");
check(supportHtml.includes("https://github.com/ukir-rrrr/winzip-jp/issues"), "サポートに Issues のリンクがありません");
check(!/Support|Contact us|Click here/i.test(supportHtml), "サポートに英語 UI があります");
check(privacyHtml.includes('lang="ja"'), "プライバシーページが日本語ではありません");
check(privacyHtml.includes("収集しません"), "プライバシーに収集しない旨がありません");
check(privacyHtml.includes("オフライン"), "プライバシーにオフラインの旨がありません");

const skipShots = process.env.STORE_LISTING_SKIP_SHOTS === "1";
const shotDir = resolve(root, "screenshots");
const shots = skipShots ? [] : readdirSync(shotDir).filter((name) => name.endsWith(".png"));
if (!skipShots) {
  check(shots.length >= 4 && shots.length <= 5, `スクリーンショットは 4〜5 枚: ${shots.join(", ")}`);
  const shotSet = new Set(shots);
  for (const expected of EXPECTED_SHOTS) {
    check(shotSet.has(expected), `スクリーンショット ${expected} がありません`);
  }
  check(!shotSet.has("01-windows.png"), "旧ファイル 01-windows.png が残っています");
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
