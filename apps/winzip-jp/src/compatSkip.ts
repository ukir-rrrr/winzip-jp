export const COMPAT_SKIP_STORAGE_KEY = "winzip-jp.compat-confirm.skip";

export function readCompatSkip(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(COMPAT_SKIP_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeCompatSkip(skip: boolean): void {
  try {
    if (typeof localStorage === "undefined") {
      return;
    }
    if (skip) {
      localStorage.setItem(COMPAT_SKIP_STORAGE_KEY, "1");
    } else {
      localStorage.removeItem(COMPAT_SKIP_STORAGE_KEY);
    }
  } catch {
    // プライベートモードなど。記憶できなくても操作は続ける。
  }
}
