import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { COPY } from "./copy";
import { isKnownJaMessage, jaErr } from "./api";

describe("jaErr", () => {
  it("keeps known COPY and rust Japanese errors", () => {
    assert.equal(isKnownJaMessage(COPY.noFiles), true);
    assert.equal(isKnownJaMessage("追加できるファイルがありません"), true);
    assert.equal(isKnownJaMessage("入力が見つかりません: /tmp/a"), true);
    assert.equal(jaErr(COPY.browserCreate, new Error("別の処理が実行中です")).message, "別の処理が実行中です");
  });

  it("drops English and mixed messages that only contain a Japanese character", () => {
    assert.equal(isKnownJaMessage("Failed to invoke 作成"), false);
    assert.equal(isKnownJaMessage("TypeError: Cannot read properties of undefined"), false);
    assert.equal(jaErr(COPY.noDest, new Error("os error 2: No such file")).message, COPY.noDest);
    assert.equal(jaErr(COPY.noFiles, new Error("dialog failed 作成")).message, COPY.noFiles);
  });
});
