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

describe("画面 S1–S9b", () => {
  it("S1 起動: 作成ドロップゾーンと VoiceOver 名", async () => {
    await openScene();
    const create = $("[data-testid='tab-create']");
    const extract = $("[data-testid='tab-extract']");
    await expect(create).toHaveAttribute("aria-label", "作成");
    await expect(extract).toHaveAttribute("aria-label", "解凍");
    await expect(create).toHaveAttribute("aria-selected", "true");
    await expect($("[data-testid='dropzone-title']")).toHaveText(
      "ファイルをドロップして ZIP を作成",
    );
    const body = await $("body").getText();
    expect(body).toContain("Windows で開ける形式（CP932）で保存します");
    expect(body).not.toContain("Windows のエクスプローラーでは開けません");
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

  it("S9 AES オプション: エクスプローラー警告", async () => {
    await openScene();
    await $("[data-testid='aes-toggle']").click();
    await expect($("[data-testid='aes-toggle']")).toHaveAttribute("aria-checked", "true");
    await expect($("[data-testid='aes-warning']")).toHaveText(
      "パスワード付き ZIP は、Windows のエクスプローラーでは開けません。7-Zip などが必要です。",
    );
    const body = await $("body").getText();
    expect(body).toContain("Windows のエクスプローラーでは開けません");
    expect(body).toContain("先方も UTF-8 前提のツールに限ります");
    await shot("s9");
  });

  it("S9b 分割オプション: エクスプローラー警告", async () => {
    await openScene();
    await $("[data-testid='split-toggle']").click();
    await expect($("[data-testid='split-toggle']")).toHaveAttribute("aria-checked", "true");
    await expect($("[data-testid='split-warning']")).toHaveText(
      "分割した ZIP もエクスプローラーでは開けません。",
    );
    const body = await $("body").getText();
    expect(body).toContain("分割した ZIP もエクスプローラーでは開けません。");
    expect(body).toContain("8 MB");
    expect(body).toContain("10 MB");
    expect(body).toContain("25 MB");
    expect(body).toContain("100 MB");
    await shot("s9b");
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
