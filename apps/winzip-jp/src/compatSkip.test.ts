import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { COMPAT_SKIP_STORAGE_KEY, readCompatSkip, writeCompatSkip } from "./compatSkip";

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

describe("互換警告の次回から表示しない", () => {
  it("未設定なら確認する", () => {
    installMemoryStorage();
    assert.equal(readCompatSkip(), false);
  });

  it("記録したら再起動後も出さない", () => {
    installMemoryStorage();
    writeCompatSkip(true);
    assert.equal(localStorage.getItem(COMPAT_SKIP_STORAGE_KEY), "1");
    assert.equal(readCompatSkip(), true);
  });

  it("localStorage が投げても落ちない", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("blocked");
      },
    });
    assert.equal(readCompatSkip(), false);
    writeCompatSkip(true);
  });
});
