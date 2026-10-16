import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { installApi, jaErr, resetApi } from "./api";
import { COMPAT_SKIP_STORAGE_KEY, writeCompatSkip } from "./compatSkip";
import { COPY } from "./copy";
import { S3_PLAN } from "./e2e/scenes";
import { useAppStore } from "./store";
import type { WriteReport } from "./types";

afterEach(() => {
  resetApi();
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.removeItem(COMPAT_SKIP_STORAGE_KEY);
    }
  } catch {
    // テスト用ストレージが無い場合もある
  }
  useAppStore.setState({
    mode: "create",
    dropped: [],
    plan: null,
    replacements: {},
    sheet: "none",
    list: null,
    archivePath: null,
    encodingOverride: null,
    aesEnabled: false,
    password: "",
    splitEnabled: false,
    splitPresetMb: 10,
    splitCustomMb: "",
    job: null,
    confirming: false,
    compatConfirm: false,
    error: "",
    epoch: useAppStore.getState().epoch + 1,
  });
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

  it("末尾空白の置換は trim して送る", async () => {
    let sent: Record<string, string> | undefined;
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.replacements;
        return { dest: req.paths[0] ?? "", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().updateReplacement("髙.png", "高.png ");
    await useAppStore.getState().confirmAndCreate();
    assert.equal(sent?.["髙.png"], "高.png");
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

  it("CP932 に無い置換は確定前に作成しない", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      async checkNames() {
        return false;
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().updateReplacement("髙.png", "é.png");
    await useAppStore.getState().confirmAndCreate();
    assert.equal(useAppStore.getState().error, COPY.invalidReplacement);
    assert.equal(useAppStore.getState().sheet, "open");
    assert.equal(useAppStore.getState().job, null);
  });

  it("確定の連打では作成を 1 回だけ始める", async () => {
    let finishCheck!: (ok: boolean) => void;
    let createCalls = 0;
    let pickCalls = 0;
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      checkNames: () =>
        new Promise((resolve) => {
          finishCheck = resolve;
        }),
      async pickSaveZip() {
        pickCalls += 1;
        return "/tmp/e2e/送り.zip";
      },
      async createArchive() {
        createCalls += 1;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const first = useAppStore.getState().confirmAndCreate();
    const second = useAppStore.getState().confirmAndCreate();
    finishCheck(true);
    await Promise.all([first, second]);
    assert.equal(pickCalls, 1);
    assert.equal(createCalls, 1);
  });

  it("確定中の置き換え変更は作成に渡さない", async () => {
    let finishCheck!: (ok: boolean) => void;
    let sent: Record<string, string> | undefined;
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      checkNames: () =>
        new Promise((resolve) => {
          finishCheck = resolve;
        }),
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.replacements;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const pending = useAppStore.getState().confirmAndCreate();
    useAppStore.getState().updateReplacement("髙.png", "a:b.png");
    assert.equal(useAppStore.getState().replacements["髙.png"], "高.png");
    finishCheck(true);
    await pending;
    assert.equal(sent?.["髙.png"], "高.png");
  });

  it("確定後の古い checkNames はシートを戻さない", async () => {
    const checks: ((ok: boolean) => void)[] = [];
    let sent: Record<string, string> | undefined;
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      checkNames: () =>
        new Promise((resolve) => {
          checks.push(resolve);
        }),
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.replacements;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().updateReplacement("髙.png", "高.png");
    const pending = useAppStore.getState().confirmAndCreate();
    assert.equal(checks.length, 2);
    checks[0](false);
    await Promise.resolve();
    assert.equal(useAppStore.getState().error, "");
    checks[1](true);
    await pending;
    assert.equal(sent?.["髙.png"], "高.png");
    assert.notEqual(useAppStore.getState().error, COPY.invalidReplacement);
  });

  it("絵文字の置換では作成しない", async () => {
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().updateReplacement("髙.png", "😀.png");
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

  it("確定中にやめると作成しない", async () => {
    let finishCheck!: (ok: boolean) => void;
    installApi({
      async planArchive() {
        return S3_PLAN;
      },
      checkNames: () =>
        new Promise((resolve) => {
          finishCheck = resolve;
        }),
      async pickSaveZip() {
        throw new Error("呼ばれてはいけない");
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const pending = useAppStore.getState().confirmAndCreate();
    useAppStore.getState().dismissSheet();
    finishCheck(true);
    await pending;
    assert.equal(useAppStore.getState().sheet, "dismissed");
    assert.equal(useAppStore.getState().job, null);
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
        throw new Error(COPY.needConfirm);
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    await useAppStore.getState().confirmAndCreate();
    assert.equal(useAppStore.getState().sheet, "open");
    assert.equal(useAppStore.getState().job, null);
    assert.equal(useAppStore.getState().error, COPY.invalidReplacement);
    assert.notEqual(useAppStore.getState().error, COPY.needConfirm);
  });

  it("取り込み開始で古い plan を消す", async () => {
    let finish!: (plan: typeof S3_PLAN) => void;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/old"]);
    assert.equal(useAppStore.getState().plan?.entries[0], "keep.txt");
    installApi({
      planArchive: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    });
    const next = useAppStore.getState().ingestDropped(["/tmp/e2e/new"]);
    assert.equal(useAppStore.getState().plan, null);
    assert.deepEqual(useAppStore.getState().dropped, ["/tmp/e2e/new"]);
    finish(S3_PLAN);
    await next;
    assert.equal(useAppStore.getState().sheet, "open");
  });

  it("進捗イベントの done は無視する", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      pickSaveZip: () => new Promise(() => undefined),
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const create = useAppStore.getState().createZip();
    useAppStore.getState().setProgress({
      phase: "done",
      currentEntry: null,
      entriesDone: 1,
      entriesTotal: 1,
      bytesDone: 1,
      bytesTotal: 1,
    });
    assert.equal(useAppStore.getState().job?.progress.phase, "starting");
    void create;
  });

  it("ジョブ中のやり直すは無視する", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      pickSaveZip: () => new Promise(() => undefined),
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    const create = useAppStore.getState().createZip();
    useAppStore.getState().resetWork();
    assert.ok(useAppStore.getState().job);
    assert.deepEqual(useAppStore.getState().dropped, ["/tmp/e2e/in"]);
    void create;
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

describe("AES オプション", () => {
  it("既定はオフでパスワードを送らない", async () => {
    let sent: string | null | undefined = "unset";
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.password ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    assert.equal(useAppStore.getState().aesEnabled, false);
    await useAppStore.getState().createZip();
    assert.equal(sent, null);
  });

  it("空パスワードでは作成しない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().error, COPY.emptyPassword);
    assert.equal(useAppStore.getState().job, null);
  });

  it("オンなら UTF-8 パスワードを渡す", async () => {
    let sent: string | null | undefined;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.password ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    useAppStore.getState().setPassword("パスワード");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, true);
    assert.equal(sent, undefined);
    await useAppStore.getState().confirmCompat(true, false);
    assert.equal(sent, "パスワード");
    assert.equal(useAppStore.getState().password, "");
    assert.equal(useAppStore.getState().job?.usedPassword, true);
  });

  it("パスワードなしで作成するとパスワードを送らない", async () => {
    let sent: string | null | undefined = "unset";
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.password ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    useAppStore.getState().setPassword("パスワード");
    await useAppStore.getState().createZip();
    await useAppStore.getState().confirmCompat(false, false);
    assert.equal(sent, null);
    assert.equal(useAppStore.getState().aesEnabled, false);
    assert.equal(useAppStore.getState().job?.usedPassword, false);
  });

  it("空パスワードでは確認に進まない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, false);
    assert.equal(useAppStore.getState().error, COPY.emptyPassword);
  });
});

describe("互換確認", () => {
  it("次回から表示しないを記録したら確認を飛ばす", async () => {
    const memory = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem(key: string) {
          return memory.has(key) ? (memory.get(key) ?? null) : null;
        },
        setItem(key: string, value: string) {
          memory.set(key, value);
        },
        removeItem(key: string) {
          memory.delete(key);
        },
      },
    });
    writeCompatSkip(true);
    assert.equal(localStorage.getItem(COMPAT_SKIP_STORAGE_KEY), "1");
    let sent: string | null | undefined;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.password ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    useAppStore.getState().setPassword("パスワード");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, false);
    assert.equal(sent, "パスワード");
    writeCompatSkip(false);
  });

  it("確認をやめると作成しない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    useAppStore.getState().setPassword("パスワード");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, true);
    useAppStore.getState().dismissCompatConfirm();
    assert.equal(useAppStore.getState().compatConfirm, false);
    assert.equal(useAppStore.getState().job, null);
    assert.equal(useAppStore.getState().aesEnabled, true);
  });

  it("分割だけなら確認せず作る", async () => {
    let sent: number | null | undefined;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.splitBytes ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setSplitEnabled(true);
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, false);
    assert.equal(sent, 10 * 1024 * 1024);
    assert.equal(useAppStore.getState().job?.usedSplit, true);
    assert.equal(useAppStore.getState().job?.usedPassword, false);
  });

  it("パスワードと分割が両方オンなら確認を共有する", async () => {
    let password: string | null | undefined;
    let split: number | null | undefined;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        password = req.password ?? null;
        split = req.splitBytes ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setAesEnabled(true);
    useAppStore.getState().setPassword("秘密");
    useAppStore.getState().setSplitEnabled(true);
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().compatConfirm, true);
    assert.equal(password, undefined);
    await useAppStore.getState().confirmCompat(true, false);
    assert.equal(password, "秘密");
    assert.equal(split, 10 * 1024 * 1024);
    assert.equal(useAppStore.getState().job?.usedPassword, true);
    assert.equal(useAppStore.getState().job?.usedSplit, true);
  });
});

describe("分割オプション", () => {
  it("既定はオフで splitBytes を送らない", async () => {
    let sent: number | null | undefined = "unset" as unknown as number;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.splitBytes ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    assert.equal(useAppStore.getState().splitEnabled, false);
    await useAppStore.getState().createZip();
    assert.equal(sent, null);
  });

  it("10 MB プリセットをバイトで送る", async () => {
    let sent: number | null | undefined;
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async pickSaveZip() {
        return "/tmp/e2e/送り.zip";
      },
      async createArchive(req) {
        sent = req.splitBytes ?? null;
        return { dest: "/tmp/e2e/送り.zip", entriesWritten: 1, bytesWritten: 1, excluded: [] };
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setSplitEnabled(true);
    await useAppStore.getState().createZip();
    assert.equal(sent, 10 * 1024 * 1024);
  });

  it("任意サイズが空なら作成しない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setSplitEnabled(true);
    useAppStore.getState().setSplitPresetMb("custom");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().error, COPY.splitCustomNeed);
    assert.equal(useAppStore.getState().job, null);
  });

  it("任意サイズが不正なら作成しない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setSplitEnabled(true);
    useAppStore.getState().setSplitPresetMb("custom");
    useAppStore.getState().setSplitCustomMb("1.5");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().error, COPY.splitCustomInvalid);
    assert.equal(useAppStore.getState().job, null);
  });

  it("任意サイズが Infinity になる値なら作成しない", async () => {
    installApi({
      async planArchive() {
        return { entries: ["keep.txt"], issues: [], excluded: [], destNameHint: "keep.zip" };
      },
      async createArchive() {
        throw new Error("呼ばれてはいけない");
      },
    });
    await useAppStore.getState().ingestDropped(["/tmp/e2e/in"]);
    useAppStore.getState().setSplitEnabled(true);
    useAppStore.getState().setSplitPresetMb("custom");
    useAppStore.getState().setSplitCustomMb("1e308");
    await useAppStore.getState().createZip();
    assert.equal(useAppStore.getState().error, COPY.splitCustomInvalid);
    assert.equal(useAppStore.getState().job, null);
  });
});

describe("展開エラー文言", () => {
  it("CRC 不一致は jaErr 経由で英語頭字語を出さない", async () => {
    installApi({
      async pickExtractDir() {
        return "/tmp/e2e/展開";
      },
      async extractArchive() {
        throw jaErr(COPY.failed, new Error("ZIP が壊れています: a.txt の CRC が一致しません"));
      },
    });
    useAppStore.setState({
      mode: "extract",
      archivePath: "/tmp/e2e/crc.zip",
      dropped: ["/tmp/e2e/crc.zip"],
      confirming: false,
      job: null,
      sheet: "none",
      list: {
        entries: [{ nameHex: "612E747874", displayName: "a.txt", isDir: false, uncompressedSize: 5, gpbf: 0 }],
        encoding: "cp932",
        confidence: 1,
        ambiguous: false,
      },
      error: "",
    });
    await useAppStore.getState().extractZip();
    assert.equal(
      useAppStore.getState().error,
      "ZIP が壊れています: a.txt の中身が壊れています（チェックサムが一致しません）",
    );
    assert.equal(useAppStore.getState().error.includes("CRC"), false);
    assert.equal(useAppStore.getState().job, null);
  });

  it("cancelJob の失敗で生の英語を出さない", async () => {
    installApi({
      async cancelJob() {
        throw jaErr(COPY.failed, new Error("os error 2: No such file"));
      },
    });
    useAppStore.setState({
      confirming: false,
      job: {
        kind: "extract",
        dest: null,
        usedPassword: false,
        usedSplit: false,
        progress: {
          phase: "writing",
          currentEntry: null,
          entriesDone: 0,
          entriesTotal: 1,
          bytesDone: 0,
          bytesTotal: 1,
        },
      },
      error: "",
    });
    await useAppStore.getState().cancel();
    assert.equal(useAppStore.getState().error, COPY.cancelled);
    assert.equal(useAppStore.getState().job, null);
  });
});
