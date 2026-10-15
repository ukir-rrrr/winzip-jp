import type { ListReport, PlanDto, ProgressDto } from "../types";

export const S2_PLAN: PlanDto = {
  entries: ["keep.txt", "nested/", "nested/ok.txt"],
  issues: [],
  excluded: [".DS_Store", "nested/.DS_Store", "__MACOSX"],
  destNameHint: "keep.zip",
};

export const S3_PLAN: PlanDto = {
  entries: ["髙.png", "﨑.txt"],
  issues: [
    {
      relativePath: "髙.png",
      originalChar: "髙",
      suggested: "高",
      kind: "compatIdeograph",
    },
    {
      relativePath: "﨑.txt",
      originalChar: "﨑",
      suggested: "崎",
      kind: "compatIdeograph",
    },
  ],
  excluded: [],
  destNameHint: "髙.zip",
};

export const S6_LIST: ListReport = {
  entries: [
    {
      nameHex: "82AA2E747874",
      displayName: "が.txt",
      isDir: false,
      uncompressedSize: 4,
      gpbf: 0,
    },
  ],
  encoding: "cp932",
  confidence: 0.8,
  ambiguous: false,
};

export const S7_LIST: ListReport = {
  entries: [
    {
      nameHex: "82AA2E747874",
      displayName: "\uFFFD\uFFFD.txt",
      isDir: false,
      uncompressedSize: 4,
      gpbf: 0,
    },
  ],
  encoding: "utf8",
  confidence: 0.8,
  ambiguous: false,
};

export const STARTING: ProgressDto = {
  phase: "starting",
  currentEntry: null,
  entriesDone: 0,
  entriesTotal: 2,
  bytesDone: 0,
  bytesTotal: 120,
};

export const E2E_CREATE_DEST = "/tmp/e2e/送り.zip";
export const E2E_EXTRACT_DEST = "/tmp/e2e/展開";
