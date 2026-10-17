import type { ListReport, PlanDto } from "../types";

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

export const E2E_CREATE_DEST = "/tmp/e2e/送り.zip";
export const E2E_EXTRACT_DEST = "/tmp/e2e/展開";

export const STORE_CREATE_PLAN: PlanDto = {
  entries: [
    "見積書_2026年10月.xlsx",
    "請求書/株式会社サンプル.pdf",
    "写真/現場_大阪.jpg",
    "議事録_10月定例.docx",
    "注文書/部品リスト.csv",
  ],
  issues: [],
  excluded: [],
  destNameHint: "見積一式.zip",
};

export const STORE_JUNK_PLAN: PlanDto = {
  entries: [
    "報告書.docx",
    "資料/議事録.pdf",
    "資料/添付.xlsx",
    "写真/現場.jpg",
  ],
  issues: [],
  excluded: [".DS_Store", "資料/.DS_Store", "__MACOSX"],
  destNameHint: "報告書.zip",
};

export const STORE_EXTRACT_LIST: ListReport = {
  entries: [
    {
      nameHex: "8ca990cf8f915f32303236944e31308c8e2e786c7378",
      displayName: "見積書_2026年10月.xlsx",
      isDir: false,
      uncompressedSize: 18432,
      gpbf: 0,
    },
    {
      nameHex: "90bf8b818f912f8a948eae89ef8ed0835483938376838b2e706466",
      displayName: "請求書/株式会社サンプル.pdf",
      isDir: false,
      uncompressedSize: 9024,
      gpbf: 0,
    },
    {
      nameHex: "8eca905e2f8cbb8fea5f91e58de32e6a7067",
      displayName: "写真/現場_大阪.jpg",
      isDir: false,
      uncompressedSize: 240128,
      gpbf: 0,
    },
    {
      nameHex: "8b638e96985e5f31308c8e92e897e12e646f6378",
      displayName: "議事録_10月定例.docx",
      isDir: false,
      uncompressedSize: 22016,
      gpbf: 0,
    },
  ],
  encoding: "cp932",
  confidence: 0.95,
  ambiguous: false,
};
