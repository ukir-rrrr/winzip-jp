/// <reference types="@wdio/globals/types" />
/// <reference types="@wdio/mocha-framework" />
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const artifacts = resolve(import.meta.dirname, "../artifacts");

async function shot(id: string): Promise<void> {
  mkdirSync(artifacts, { recursive: true });
  await browser.saveScreenshot(resolve(artifacts, `${id}.png`));
}

async function openScene(id?: string): Promise<void> {
  await browser.url(id ? `/?scene=${id}` : "/");
  await $("[data-testid='app-shell']").waitForDisplayed();
}

describe("画面 S1–S10", () => {
  it("S1 起動: 作成ドロップゾーンと VoiceOver 名", async () => {
    await openScene();
    const create = $("[data-testid='tab-create']");
    const extract = $("[data-testid='tab-extract']");
    await expect(create).toHaveAttribute("aria-label", "作成");
    await expect(extract).toHaveAttribute("aria-label", "解凍");
    await expect(create).toHaveAttribute("title", "Windows で開ける ZIP を作ります。");
    await expect(extract).toHaveAttribute("title", "文字化けした ZIP を正しく展開します。");
    await expect($("[data-testid='app-title']")).toHaveText("文字化けしないZIP");
    await expect($("[data-testid='btn-guide']")).toBeDisplayed();
    await expect($("[data-testid='btn-guide']")).toHaveAttribute(
      "aria-label",
      "文字化けしないZIP の使い方",
    );
    await expect($("[data-testid='guide']")).not.toBeExisting();
    await expect(create).toHaveAttribute("aria-selected", "true");
    await expect($("[data-testid='dropzone-title']")).toHaveText(
      "ファイルをドロップして ZIP を作成",
    );
    const body = await $("body").getText();
    expect(body).toContain("文字化けしないZIP");
    expect(body).not.toContain("送りZIP");
    expect(body).toContain("Windows で開ける形式（CP932）で保存します");
    expect(body).not.toContain("Windows のエクスプローラーでは開けません");
    expect(body).not.toContain("Windows 標準では開けません");
    await expect($("[data-testid='aes-toggle']")).toHaveAttribute("aria-checked", "false");
    await expect($("[data-testid='split-toggle']")).toHaveAttribute("aria-checked", "false");
    await shot("s1");
  });

  it("S2 フィクスチャ一覧: .DS_Store が無い", async () => {
    await openScene("s2");
    const text = await $("[data-testid='file-list']").getText();
    expect(text).toContain("keep.txt");
    expect(text).toContain("nested/ok.txt");
    expect(text).not.toContain(".DS_Store");
    expect(text).not.toContain("__MACOSX");
    await shot("s2");
  });

  it("S3 不能文字シート: 髙 → 高", async () => {
    await openScene("s3");
    const sheet = $("[data-testid='unmappable-sheet']");
    await expect(sheet).toBeDisplayed();
    await expect($("[data-testid='unmappable-title']")).toHaveText(
      "次の名前は Windows で使えない文字を含むため、置き換えて保存します",
    );
    const text = await sheet.getText();
    expect(text).toContain("髙.png");
    expect(text).toContain("髙");
    await expect($("[data-testid='repl-髙.png']")).toHaveValue("高.png");
    await expect($("[data-testid='repl-﨑.txt']")).toHaveValue("崎.txt");
    await shot("s3");
  });

  it("S4 置換確定から進捗", async () => {
    await openScene("s3");
    await $("[data-testid='btn-confirm-replace']").click();
    await $("[data-testid='progress-overlay']").waitForDisplayed();
    await expect($("[data-testid='progress-text']")).toHaveText(
      expect.stringContaining("開始しています"),
    );
    await shot("s4");
  });

  it("S5 解凍モードへ切替", async () => {
    await openScene();
    await $("[data-testid='tab-extract']").click();
    await expect($("[data-testid='tab-extract']")).toHaveAttribute("aria-selected", "true");
    await expect($("[data-testid='dropzone-title']")).toHaveText(
      "文字化けした ZIP をドロップして解凍",
    );
    await expect($("[data-testid='app-shell']")).toHaveAttribute("data-mode", "extract");
    await shot("s5");
  });

  it("S6 CP932 ZIP の一覧", async () => {
    await openScene("s6");
    await expect($("[data-testid='enc-cp932']")).toHaveAttribute("aria-checked", "true");
    const text = await $("[data-testid='file-list']").getText();
    expect(text).toContain("が.txt");
    const body = await $("body").getText();
    expect(body).toContain("CP932");
    await shot("s6");
  });

  it("S7 UTF-8 切替で一覧が化ける", async () => {
    await openScene("s6");
    await $("[data-testid='enc-utf8']").click();
    await expect($("[data-testid='enc-utf8']")).toHaveAttribute("aria-checked", "true");
    const text = await $("[data-testid='file-list']").getText();
    expect(text).not.toContain("が.txt");
    expect(text).toContain(".txt");
    await shot("s7");
  });

  it("S8 展開先をコマンド経由で確認", async () => {
    await openScene("s6");
    await $("[data-testid='btn-extract']").click();
    await $("[data-testid='progress-overlay']").waitForDisplayed();
    await $("[data-testid='extract-dest']").waitForDisplayed();
    await expect($("[data-testid='extract-dest']")).toHaveText(
      expect.stringContaining("/tmp/e2e/展開"),
    );
    const dest = await browser.execute(() => window.__WINZIP_E2E__?.getState().lastDest ?? "");
    expect(dest).toContain("/tmp/e2e/展開");
    await shot("s8");
  });

  it("S9 AES オプション: 作成ボタン近くの警告", async () => {
    await openScene("s2");
    await expect($("[data-testid='aes-toggle']")).toHaveAttribute("aria-checked", "false");
    await $("[data-testid='aes-toggle']").click();
    await expect($("[data-testid='aes-toggle']")).toHaveAttribute("aria-checked", "true");
    await expect($("[data-testid='aes-toggle']")).toHaveAttribute(
      "title",
      "ZIP にパスワードを付けます。エクスプローラーでは開けません。",
    );
    await expect($("[data-testid='aes-password']")).toBeDisplayed();
    const alert = $("[data-testid='aes-warning']");
    await expect(alert).toBeDisplayed();
    const text = await alert.getText();
    expect(text).toContain("相手が 7-Zip などを持っていないと開けません（Windows 標準では開けません）");
    expect(text).toContain("相手が Windows 標準で開くなら、パスワードはオフにしてください");
    const body = await $("body").getText();
    expect(body).toContain("先方も UTF-8 前提のツールに限ります");
    await shot("s9");
  });

  it("S9b 分割オプション: 作成ボタン近くの警告", async () => {
    await openScene("s2");
    await $("[data-testid='split-toggle']").click();
    await expect($("[data-testid='split-toggle']")).toHaveAttribute("aria-checked", "true");
    await expect($("[data-testid='split-toggle']")).toHaveAttribute(
      "title",
      "指定サイズで分けます。エクスプローラーでは開けません。",
    );
    const alert = $("[data-testid='split-warning']");
    await expect(alert).toBeDisplayed();
    const text = await alert.getText();
    expect(text).toContain("分割した ZIP は Windows 標準では開けません。7-Zip などが必要です。");
    expect(text).toContain("相手が Windows 標準で開くなら、分割はオフにしてください");
    const body = await $("body").getText();
    expect(body).toContain("8 MB");
    expect(body).toContain("10 MB");
    expect(body).toContain("25 MB");
    expect(body).toContain("100 MB");
    await shot("s9b");
  });

  it("S9d パスワード付き作成の確認", async () => {
    await openScene("s9d");
    await $("[data-testid='aes-toggle']").click();
    await $("[data-testid='aes-password']").setValue("秘密");
    await $("[data-testid='btn-create']").click();
    const modal = $("[data-testid='compat-confirm']");
    await expect(modal).toBeDisplayed();
    const text = await modal.getText();
    expect(text).toContain("相手が 7-Zip などを持っていないと開けません（Windows 標準では開けません）");
    expect(text).toContain("相手が Windows 標準で開くなら、パスワードはオフにしてください");
    expect(text).toContain("このまま作成");
    expect(text).toContain("パスワードなしで作成");
    expect(text).toContain("やめる");
    expect(text).toContain("次回から表示しない");
    await shot("s9d");
    await $("[data-testid='btn-create-anyway']").click();
    await $("[data-testid='compat-reminder']").waitForDisplayed();
    await expect($("[data-testid='compat-reminder']")).toHaveText(
      "受け取る人に 7-Zip が必要と伝えてください",
    );
  });

  it("S9c パスワードと分割を同時に開く: 入力が見え、選ぶボタンは隠れない", async () => {
    await openScene();
    await $("[data-testid='aes-toggle']").click();
    await $("[data-testid='split-toggle']").click();
    const both = await $("[data-testid='aes-warning']").getText();
    expect(both).toContain("相手が 7-Zip などを持っていないと開けません（Windows 標準では開けません）");
    expect(both).toContain("相手が Windows 標準で開くなら、分割はオフにしてください");
    const input = $("[data-testid='aes-password']");
    const pick = $("[data-testid='btn-pick-files']");
    await expect(input).toBeDisplayed();
    await expect(pick).toBeDisplayed();
    await expect($("[data-testid='aes-password-toggle']")).toHaveText("表示");
    await expect($("[data-testid='aes-password-toggle']")).toHaveAttribute(
      "aria-label",
      "パスワードを表示",
    );
    const layout = await browser.execute(() => {
      const field = document.querySelector("[data-testid='aes-password']");
      const button = document.querySelector("[data-testid='btn-pick-files']");
      const banners = document.querySelector("[data-testid='option-banners']");
      if (
        !(field instanceof HTMLElement) ||
        !(button instanceof HTMLElement) ||
        !(banners instanceof HTMLElement)
      ) {
        return { ok: false };
      }
      button.scrollIntoView({ block: "nearest" });
      const fieldBox = field.getBoundingClientRect();
      const buttonBox = button.getBoundingClientRect();
      const bannerBox = banners.getBoundingClientRect();
      const overlap = !(
        buttonBox.bottom <= bannerBox.top + 0.5 ||
        buttonBox.top >= bannerBox.bottom - 0.5 ||
        buttonBox.right <= bannerBox.left ||
        buttonBox.left >= bannerBox.right
      );
      return {
        ok:
          fieldBox.width >= 80 &&
          fieldBox.height >= 24 &&
          buttonBox.width >= 60 &&
          buttonBox.height >= 24 &&
          buttonBox.top >= bannerBox.bottom - 1 &&
          !overlap,
      };
    });
    expect(layout.ok).toBe(true);
    await browser.execute(() => {
      window.scrollTo(0, 0);
      document.documentElement.scrollTop = 0;
      document.body.scrollTop = 0;
      document.querySelector("[data-testid='content']")?.scrollTo(0, 0);
    });
    await shot("s9c");
    await input.setValue("秘密");
    await $("[data-testid='aes-password-toggle']").click();
    await expect(input).toHaveAttribute("type", "text");
    await expect(input).toHaveValue("秘密");
    await expect($("[data-testid='aes-password-toggle']")).toHaveText("表示");
    await expect($("[data-testid='aes-password-toggle']")).toHaveAttribute("aria-pressed", "true");
  });

  it("S10 初回ガイド: 3 手順と解凍の説明", async () => {
    await openScene("guide");
    const guide = $("[data-testid='guide']");
    await expect(guide).toBeDisplayed();
    await expect(guide).toHaveAttribute("aria-label", "文字化けしないZIP の使い方");
    await expect($("[data-testid='app-title']")).toHaveText("文字化けしないZIP");
    const text = await guide.getText();
    expect(text).toContain("ファイルをドロップする");
    expect(text).toContain("必要ならパスワードや分割を付ける");
    expect(text).toContain("保存先を選ぶ");
    expect(text).toContain("解凍タブでは、Windows から来た文字化け ZIP を直せます。");
    await expect($("[data-testid='btn-guide']")).toHaveAttribute("aria-pressed", "true");
    await expect($("[data-testid='btn-guide']")).toHaveAttribute(
      "aria-label",
      "文字化けしないZIP の使い方",
    );
    const body = await $("body").getText();
    expect(body).toContain("文字化けしないZIP");
    expect(body).not.toContain("送りZIP");
    await shot("s10");
    await $("[data-testid='guide-dismiss']").click();
    await expect(guide).not.toBeDisplayed();
    await $("[data-testid='btn-guide']").click();
    await expect($("[data-testid='guide']")).toBeDisplayed();
  });

  it("展開 CRC 不一致は英語頭字語を出さない", async () => {
    await openScene("crc");
    await expect($("[data-testid='status-text']")).toHaveText(
      "ZIP が壊れています: a.txt の中身が壊れています（チェックサムが一致しません）",
    );
    const body = await $("body").getText();
    expect(body).not.toContain("CRC");
    expect(body).not.toContain("EOCD");
    await shot("crc");
  });
});
