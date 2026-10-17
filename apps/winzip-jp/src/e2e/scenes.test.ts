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

  it("解凍一覧は CP932 で日本語名を出す", () => {
    assert.equal(STORE_EXTRACT_LIST.encoding, "cp932");
    assert.ok(STORE_EXTRACT_LIST.entries.some((entry) => entry.displayName.includes("見積書")));
  });
});
