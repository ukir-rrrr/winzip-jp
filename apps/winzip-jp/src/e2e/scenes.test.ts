import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { STORE_CREATE_PLAN, STORE_EXTRACT_LIST, STORE_JUNK_PLAN } from "./scenes";

describe("ストア用シーン", () => {
  it("作成一覧は日本語名で、ジャンクを含まない", () => {
    assert.ok(STORE_CREATE_PLAN.entries.some((name) => name.includes("見積書")));
    assert.ok(!STORE_CREATE_PLAN.entries.some((name) => name.includes(".DS_Store")));
  });

  it("除外シーンは Finder ジャンクを excluded にだけ持つ", () => {
    assert.ok(STORE_JUNK_PLAN.excluded.includes(".DS_Store"));
    assert.ok(STORE_JUNK_PLAN.excluded.includes("__MACOSX"));
    assert.ok(!STORE_JUNK_PLAN.entries.some((name) => name.includes(".DS_Store")));
    assert.ok(!STORE_JUNK_PLAN.entries.some((name) => name.includes("__MACOSX")));
  });

  it("解凍一覧の nameHex は表示名の CP932 で、bit 11 は落ちている", () => {
    const cp932Hex: Record<string, string> = {
      "見積書_2026年10月.xlsx": "8ca990cf8f915f32303236944e31308c8e2e786c7378",
      "請求書/株式会社サンプル.pdf": "90bf8b818f912f8a948eae89ef8ed0835483938376838b2e706466",
      "写真/現場_大阪.jpg": "8eca905e2f8cbb8fea5f91e58de32e6a7067",
      "議事録_10月定例.docx": "8b638e96985e5f31308c8e92e897e12e646f6378",
    };
    assert.equal(STORE_EXTRACT_LIST.encoding, "cp932");
    assert.equal(STORE_EXTRACT_LIST.entries.length, Object.keys(cp932Hex).length);
    for (const entry of STORE_EXTRACT_LIST.entries) {
      assert.equal(entry.nameHex, cp932Hex[entry.displayName], entry.displayName);
      assert.equal(entry.gpbf & 0x0800, 0, entry.displayName);
    }
  });
});
