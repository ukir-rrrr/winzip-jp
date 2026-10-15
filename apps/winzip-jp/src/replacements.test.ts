import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  defaultReplacements,
  isValidReplacement,
  normalizeReplacement,
  normalizeReplacements,
  replacementFieldId,
  replacementIsReady,
  replacementRows,
  suggestedPath,
} from "./replacements";
import type { Unmappable } from "./types";

const taka: Unmappable = {
  relativePath: "髙.png",
  originalChar: "髙",
  suggested: "高",
  kind: "compatIdeograph",
};

const saki: Unmappable = {
  relativePath: "﨑.txt",
  originalChar: "﨑",
  suggested: "崎",
  kind: "compatIdeograph",
};

const nested: Unmappable = {
  relativePath: "dir/髙.png",
  originalChar: "髙",
  suggested: "高",
  kind: "compatIdeograph",
};

describe("suggestedPath", () => {
  it("applies the char replacement across the full path", () => {
    assert.equal(suggestedPath("髙.png", [taka]), "高.png");
    assert.equal(suggestedPath("dir/髙.png", [nested]), "dir/高.png");
  });

  it("replaces every occurrence on the same path", () => {
    const doubled: Unmappable = {
      relativePath: "髙髙.png",
      originalChar: "髙",
      suggested: "高",
      kind: "compatIdeograph",
    };
    assert.equal(suggestedPath("髙髙.png", [doubled, doubled]), "高高.png");
  });
});

describe("defaultReplacements", () => {
  it("stores the full replaced name, matching zipcore assign_names", () => {
    assert.deepEqual(defaultReplacements([taka, saki]), {
      "髙.png": "高.png",
      "﨑.txt": "崎.txt",
    });
  });
});

describe("isValidReplacement", () => {
  it("rejects empty, separators, and parent refs", () => {
    assert.equal(isValidReplacement(""), false);
    assert.equal(isValidReplacement("   "), false);
    assert.equal(isValidReplacement("/"), false);
    assert.equal(isValidReplacement("/abs"), false);
    assert.equal(isValidReplacement(".."), false);
    assert.equal(isValidReplacement("a/../b"), false);
    assert.equal(isValidReplacement("a\\b"), false);
    assert.equal(isValidReplacement("高:.png"), false);
    assert.equal(isValidReplacement("高*.png"), false);
    assert.equal(isValidReplacement("高?.png"), false);
    assert.equal(isValidReplacement("高\".png"), false);
    assert.equal(isValidReplacement("高<.png"), false);
    assert.equal(isValidReplacement("高>.png"), false);
    assert.equal(isValidReplacement("高|.png"), false);
    assert.equal(isValidReplacement("高.png."), false);
  });

  it("rejects Windows reserved device names, even with an extension", () => {
    assert.equal(isValidReplacement("CON.png"), false);
    assert.equal(isValidReplacement("con.png"), false);
    assert.equal(isValidReplacement("NUL"), false);
    assert.equal(isValidReplacement("COM1.txt"), false);
    assert.equal(isValidReplacement("LPT9.doc"), false);
    assert.equal(isValidReplacement("dir/CON.png"), false);
    assert.equal(isValidReplacement("CONSOLE.png"), true);
    assert.equal(isValidReplacement("COM10.txt"), true);
  });

  it("rejects zipcore-unmappable replacements such as emoji and 髙", () => {
    assert.equal(isValidReplacement("😀.png"), false);
    assert.equal(isValidReplacement("髙.png"), false);
    assert.equal(isValidReplacement("高.png"), true);
  });

  it("accepts CP932 symbols that sit in the misc-symbols block", () => {
    assert.equal(isValidReplacement("★.png"), true);
    assert.equal(isValidReplacement("☆.png"), true);
    assert.equal(isValidReplacement("♀.png"), true);
    assert.equal(isValidReplacement("♂.png"), true);
  });

  it("accepts a relative full name", () => {
    assert.equal(isValidReplacement("高.png"), true);
    assert.equal(isValidReplacement("dir/高.png"), true);
    assert.equal(isValidReplacement("高/"), true);
    assert.equal(isValidReplacement("dir/高/"), true);
  });

  it("trims trailing spaces before send (Windows drops them)", () => {
    assert.equal(normalizeReplacement("高.png "), "高.png");
    assert.equal(normalizeReplacement(" 高.png"), "高.png");
    assert.equal(isValidReplacement("高.png "), true);
    assert.deepEqual(normalizeReplacements({ "髙.png": "高.png " }), { "髙.png": "高.png" });
  });
});

describe("replacementIsReady", () => {
  it("rejects a value that still contains the original unmappable char", () => {
    const ligature: Unmappable = {
      relativePath: "ﬁ.txt",
      originalChar: "ﬁ",
      suggested: "fi",
      kind: "compatIdeograph",
    };
    assert.equal(replacementIsReady("ﬁ.txt", "ﬁ.txt", [ligature]), false);
    assert.equal(replacementIsReady("fi.txt", "ﬁ.txt", [ligature]), true);
  });
});

describe("replacementRows", () => {
  it("keeps one row per path", () => {
    const doubled: Unmappable = {
      relativePath: "髙髙.png",
      originalChar: "髙",
      suggested: "高",
      kind: "compatIdeograph",
    };
    assert.equal(replacementRows([doubled, doubled]).length, 1);
  });
});

describe("replacementFieldId", () => {
  it("encodes spaces so the value is a valid HTML id", () => {
    const id = replacementFieldId("my file.png");
    assert.equal(id, "repl-my%20file.png");
    assert.doesNotMatch(id, /\s/);
  });
});
