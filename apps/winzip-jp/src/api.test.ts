import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { COPY } from "./copy";
import { isKnownJaMessage, jaErr, toUiJa } from "./api";

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
    assert.equal(jaErr(COPY.failed, new Error("os error 2: No such file")).message, COPY.failed);
    assert.equal(jaErr(COPY.failed, new Error("dialog failed 作成")).message, COPY.failed);
  });

  it("rewrites EOCD in zipcore corrupt messages", () => {
    assert.equal(toUiJa("ZIP が壊れています: EOCD が見つかりません"), "ZIP が壊れています: 中央ディレクトリ終端が見つかりません");
    assert.equal(toUiJa("ZIP が壊れています: EOCD 署名がありません"), "ZIP が壊れています: 中央ディレクトリ終端の署名がありません");
    assert.equal(
      jaErr(COPY.failed, new Error("ZIP が壊れています: EOCD が見つかりません")).message,
      "ZIP が壊れています: 中央ディレクトリ終端が見つかりません",
    );
    assert.equal(
      jaErr(COPY.failed, new Error("ZIP が壊れています: EOCD 署名がありません")).message,
      "ZIP が壊れています: 中央ディレクトリ終端の署名がありません",
    );
    assert.equal(
      jaErr(COPY.failed, new Error("入力が見つかりません: /tmp/EOCD.png")).message,
      "入力が見つかりません: /tmp/EOCD.png",
    );
  });
});
