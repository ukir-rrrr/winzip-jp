import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { installApi, resetApi } from "./api";
import { COPY } from "./copy";
import { S3_PLAN } from "./e2e/scenes";
import { useAppStore } from "./store";
import type { WriteReport } from "./types";

afterEach(() => {
  resetApi();
  useAppStore.getState().setMode("create");
});

describe("replacements 契約", () => {
  it("髙.png の置換値は 高.png", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/04-replacements"]);
    assert.equal(useAppStore.getState().replacements["髙.png"], "高.png");
    assert.equal(useAppStore.getState().replacements["﨑.txt"], "崎.txt");
    assert.equal(useAppStore.getState().sheet, "open");
  });

  it("空の置換では作成しない", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().updateReplacement("髙.png", "");
    assert.equal(useAppStore.getState().error, COPY.invalidReplacement);
    await useAppStore.getState().confirmAndCreate();
    assert.equal(useAppStore.getState().sheet, "open");
    assert.equal(useAppStore.getState().job, null);
  });
});

describe("シートのやめる", () => {
  it("やめたあと作成を押すとシートが戻る", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().dismissSheet();
    assert.equal(useAppStore.getState().sheet, "dismissed");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().sheet, "open");
    assert.equal(useAppStore.getState().error, COPY.needConfirm);
  });
});

describe("世代トークン", () => {
  it("planArchive 中のモード切替は古い plan を捨てる", async () => {
    let finish!: (plan: typeof S3_PLAN) => void;
    installApi({
      planArchive: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    });
    const ingest = useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setMode("extract");
    finish(S3_PLAN);
    await ingest;
    const state = useAppStore.getState();
    assert.equal(state.mode, "extract");
    assert.equal(state.plan, null);
    assert.equal(state.sheet, "none");
  });

  it("キャンセル後に到着した run 結果を捨てる", async () => {
    let finish!: (report: WriteReport) => void;
    let started!: () => void;
    const startedP = new Promise<void>((resolve) => {
      started = resolve;
    });
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      createArchive: () => {
        started();
        return new Promise((resolve) => {
          finish = resolve;
        });
      },
      async cancelJob() {
        return undefined;
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const create = useAppStore.getState().createZip();
    await startedP;
    await useAppStore.getState().cancel();
    finish({ dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 4, excluded: [] });
    await create;
    const state = useAppStore.getState();
    assert.equal(state.job, null);
    assert.equal(state.error, COPY.cancelled);
  });

  it("setEncoding の後着は新しい list を上書きしない", async () => {
    let finishA!: (list: import("./types").ListReport) => void;
    installApi({
      async listArchive() {
        return new Promise((resolve) => {
          finishA = resolve;
        });
      },
    });
    useAppStore.setState({
      mode: "extract",
      archivePath: "/tmp/e2e/a.zip",
      list: { entries: [], encoding: "cp932", confidence: 1, ambiguous: false },
    });
    const pending = useAppStore.getState().setEncoding("utf8");
    useAppStore.getState().resetWork();
    useAppStore.setState({
      mode: "extract",
      archivePath: "/tmp/e2e/b.zip",
      list: { entries: [], encoding: "cp932", confidence: 1, ambiguous: false },
    });
    finishA({
      entries: [{ nameHex: "aa", displayName: "古い", isDir: false, uncompressedSize: 1, gpbf: 0 }],
      encoding: "utf8",
      confidence: 0.8,
      ambiguous: false,
    });
    await pending;
    assert.equal(useAppStore.getState().archivePath, "/tmp/e2e/b.zip");
    assert.equal(useAppStore.getState().list?.entries.length, 0);
  });

  it("作成失敗で不能文字シートが戻る", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive() {
        throw new Error("置き換えを確定してください");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    await useAppStore.getState().confirmAndCreate();
    assert.equal(useAppStore.getState().sheet, "open");
    assert.equal(useAppStore.getState().job, null);
    assert.match(useAppStore.getState().error, /置き換え/);
  });

  it("ジョブ中のドロップは無視する", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      pickSaveZip: () => new Promise(() => undefined),
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const create = useAppStore.getState().createZip();
    await useAppStore.getState().ingestDropped(["/tmp/e2e/other"]);
    assert.deepEqual(useAppStore.getState().dropped, ["/tmp/e2e/in"]);
    assert.ok(useAppStore.getState().job);
    void create;
  });
});
