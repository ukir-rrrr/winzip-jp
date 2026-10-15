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

export function isValidReplacement(value: string): boolean {
  const trimmed = normalizeReplacement(value);
  if (!trimmed || trimmed.includes("\\") || WIN_FORBIDDEN.test(trimmed)) {
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
    if (WIN_FORBIDDEN.test(part) || isReservedDosName(part)) {
      return false;
    }
    return !part.endsWith(".") && !part.endsWith(" ");
  });
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
