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

export function isValidReplacement(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed || trimmed.includes("\\") || trimmed.includes("\0")) {
    return false;
  }
  if (trimmed.startsWith("/") || trimmed.endsWith("/")) {
    return false;
  }
  return trimmed.split("/").every((part) => part !== "" && part !== "." && part !== "..");
}

export function replacementFieldId(relativePath: string): string {
  return `repl-${encodeURIComponent(relativePath)}`;
}
