export const GUIDE_STORAGE_KEY = "winzip-jp.guide.dismissed";

export function readGuideDismissed(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(GUIDE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeGuideDismissed(dismissed: boolean): void {
  try {
    if (typeof localStorage === "undefined") {
      return;
    }
    if (dismissed) {
      localStorage.setItem(GUIDE_STORAGE_KEY, "1");
    } else {
      localStorage.removeItem(GUIDE_STORAGE_KEY);
    }
  } catch {
    // プライベートモードなど。記憶できなくても操作は続ける。
  }
}
