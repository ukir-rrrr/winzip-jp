import type { Unmappable } from "./types";

export function suggestedPath(relativePath: string, issues: Unmappable[]): string {
  let next = relativePath;
  for (const issue of issues) {
    if (issue.relativePath !== relativePath || !issue.originalChar) {
      continue;
    }
    next = next.split(issue.originalChar).join(issue.suggested);
  }
  return next;
}

export function defaultReplacements(issues: Unmappable[]): Record<string, string> {
  const next: Record<string, string> = {};
  for (const issue of issues) {
    if (next[issue.relativePath] === undefined) {
      next[issue.relativePath] = suggestedPath(issue.relativePath, issues);
    }
  }
  return next;
}

export function normalizeReplacement(value: string): string {
  return value.trim();
}

export function normalizeReplacements(map: Record<string, string>): Record<string, string> {
  const next: Record<string, string> = {};
  for (const [key, value] of Object.entries(map)) {
    next[key] = normalizeReplacement(value);
  }
  return next;
}

const WIN_FORBIDDEN = /[:*?"<>|\x00-\x1f]/;

const WIN_RESERVED = new Set([
  "CON",
  "PRN",
  "AUX",
  "NUL",
  "COM1",
  "COM2",
  "COM3",
  "COM4",
  "COM5",
  "COM6",
  "COM7",
  "COM8",
  "COM9",
  "LPT1",
  "LPT2",
  "LPT3",
  "LPT4",
  "LPT5",
  "LPT6",
  "LPT7",
  "LPT8",
  "LPT9",
]);

function isReservedDosName(part: string): boolean {
  const stem = part.split(".")[0] ?? part;
  return WIN_RESERVED.has(stem.toUpperCase());
}

// zipcore の encode_entry_name が置換対象にする文字。確定後に UnconfirmedReplacements へ落とさない。
const FORCED_UNMAPPABLE = new Set([
  "\u{9AD9}", // 髙
  "\u{FA11}", // 﨑
  "\u{00A5}", // ¥
  "\u{203E}", // ‾
  "\u{301C}", // 〜
  "\u{2212}", // −
  "\u{00A2}", // ¢
  "\u{00A3}", // £
  "\u{00AC}", // ¬
  "\u{2016}", // ‖
  "\u{2014}", // —
]);

const EMOJI_SINGLE = new Set([
  0x00a9, 0x00ae, 0x200d, 0x203c, 0x2049, 0x20e3, 0x2122, 0x2139, 0x2328, 0x23cf, 0x24c2, 0x25b6,
  0x25c0, 0x2934, 0x2935, 0x2b50, 0x2b55, 0x3030, 0x303d, 0x3297, 0x3299, 0xfe0f,
]);

const EMOJI_RANGES: [number, number][] = [
  [0x2194, 0x2199],
  [0x21a9, 0x21aa],
  [0x231a, 0x231b],
  [0x23e9, 0x23f3],
  [0x23f8, 0x23fa],
  [0x25aa, 0x25ab],
  [0x25fb, 0x25fe],
  [0x2600, 0x27bf],
  [0x2b05, 0x2b07],
  [0x2b1b, 0x2b1c],
  [0x1f000, 0x1ffff],
  [0xe0020, 0xe007f],
];

function isEmojiCodePoint(cp: number): boolean {
  if (EMOJI_SINGLE.has(cp)) {
    return true;
  }
  return EMOJI_RANGES.some(([lo, hi]) => cp >= lo && cp <= hi);
}

function hasPolicyUnmappable(value: string): boolean {
  for (const ch of value) {
    const cp = ch.codePointAt(0);
    if (cp === undefined) {
      continue;
    }
    if (cp >= 0x10000 || FORCED_UNMAPPABLE.has(ch) || isEmojiCodePoint(cp)) {
      return true;
    }
  }
  return false;
}

export function isValidReplacement(value: string): boolean {
  const trimmed = normalizeReplacement(value);
  if (!trimmed || trimmed.includes("\\") || WIN_FORBIDDEN.test(trimmed) || hasPolicyUnmappable(trimmed)) {
    return false;
  }
  // zipcore のディレクトリ名は末尾 `/`。印としてだけ許し、成分検査の前に剥がす。
  const body = trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
  if (!body || body.startsWith("/")) {
    return false;
  }
  return body.split("/").every((part) => {
    if (!part || part === "." || part === "..") {
      return false;
    }
    if (WIN_FORBIDDEN.test(part) || isReservedDosName(part) || hasPolicyUnmappable(part)) {
      return false;
    }
    return !part.endsWith(".") && !part.endsWith(" ");
  });
}

export function replacementIsReady(
  value: string,
  relativePath: string,
  issues: Unmappable[],
): boolean {
  if (!isValidReplacement(value)) {
    return false;
  }
  const trimmed = normalizeReplacement(value);
  return !issues.some(
    (issue) => issue.relativePath === relativePath && Boolean(issue.originalChar) && trimmed.includes(issue.originalChar),
  );
}

export function replacementRows(issues: Unmappable[]): Unmappable[] {
  const seen = new Set<string>();
  const rows: Unmappable[] = [];
  for (const issue of issues) {
    if (seen.has(issue.relativePath)) {
      continue;
    }
    seen.add(issue.relativePath);
    rows.push(issue);
  }
  return rows;
}

export function replacementFieldId(relativePath: string): string {
  return `repl-${encodeURIComponent(relativePath)}`;
}
