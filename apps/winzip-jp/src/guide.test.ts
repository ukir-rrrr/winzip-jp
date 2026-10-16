import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { GUIDE_STORAGE_KEY, readGuideDismissed, writeGuideDismissed } from "./guide";

const memory = new Map<string, string>();

afterEach(() => {
  memory.clear();
});

function installMemoryStorage(): void {
  const storage = {
    getItem(key: string) {
      return memory.has(key) ? (memory.get(key) ?? null) : null;
    },
    setItem(key: string, value: string) {
      memory.set(key, value);
    },
    removeItem(key: string) {
      memory.delete(key);
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: storage,
  });
}

describe("初回ガイドの記憶", () => {
  it("未設定なら表示する", () => {
    installMemoryStorage();
    assert.equal(readGuideDismissed(), false);
  });

  it("とじたら再起動後も出さない", () => {
    installMemoryStorage();
    writeGuideDismissed(true);
    assert.equal(localStorage.getItem(GUIDE_STORAGE_KEY), "1");
    assert.equal(readGuideDismissed(), true);
  });

  it("使い方から開き直すと記憶を消す", () => {
    installMemoryStorage();
    writeGuideDismissed(true);
    writeGuideDismissed(false);
    assert.equal(localStorage.getItem(GUIDE_STORAGE_KEY), null);
    assert.equal(readGuideDismissed(), false);
  });

  it("localStorage が投げても落ちない", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("blocked");
      },
    });
    assert.equal(readGuideDismissed(), false);
    writeGuideDismissed(true);
  });
});
