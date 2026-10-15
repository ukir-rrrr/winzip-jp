import { installApi, resetApi } from "../api";
import { useAppStore } from "../store";
import {
  E2E_CREATE_DEST,
  E2E_EXTRACT_DEST,
  S2_PLAN,
  S3_PLAN,
  S6_LIST,
  S7_LIST,
  STARTING,
} from "./scenes";
import type { ListReport, NameEncoding, PlanDto, ZipApi } from "../types";

export type E2eApi = {
  reset: () => void;
  loadScene: (id: string) => void;
  getState: () => {
    mode: string;
    lastDest: string | null;
    sheetOpen: boolean;
    encoding: string | null;
    names: string[];
  };
};

function mockApi(plan: PlanDto | null, listFor: (encoding: NameEncoding | null) => ListReport | null): Partial<ZipApi> {
  return {
    async planArchive() {
      if (!plan) {
        throw new Error("追加できるファイルがありません");
      }
      return plan;
    },
    async pickFiles() {
      return plan ? ["/tmp/e2e/in"] : ["/tmp/e2e/in.zip"];
    },
    async pickSaveZip() {
      return E2E_CREATE_DEST;
    },
    async pickExtractDir() {
      return E2E_EXTRACT_DEST;
    },
    async createArchive(_req, dest) {
      useAppStore.getState().setProgress(STARTING);
      return {
        dest,
        entriesWritten: plan?.entries.length ?? 0,
        bytesWritten: 120,
        excluded: plan?.excluded ?? [],
      };
    },
    async listArchive(_path, encoding) {
      const list = listFor(encoding);
      if (!list) {
        throw new Error("ZIP が選ばれていません");
      }
      return list;
    },
    async extractArchive(req) {
      useAppStore.getState().setProgress(STARTING);
      return {
        dest: req.dest,
        entriesWritten: S6_LIST.entries.length,
        bytesWritten: 4,
      };
    },
    async cancelJob() {
      return undefined;
    },
  };
}

function loadScene(id: string): void {
  const store = useAppStore.getState();
  store.resetWork();
  resetApi();

  if (id === "s2") {
    installApi(mockApi(S2_PLAN, () => null));
    store.applyScene({
      mode: "create",
      dropped: ["/tmp/e2e/03-junk"],
      plan: S2_PLAN,
      replacementsConfirmed: true,
    });
    return;
  }
  if (id === "s3") {
    installApi(mockApi(S3_PLAN, () => null));
    store.applyScene({
      mode: "create",
      dropped: ["/tmp/e2e/04-replacements"],
      plan: S3_PLAN,
      replacements: { "髙.png": "高", "﨑.txt": "崎" },
      replacementsConfirmed: false,
      sheetOpen: true,
    });
    return;
  }
  if (id === "s4") {
    installApi(mockApi(S3_PLAN, () => null));
    store.applyScene({
      mode: "create",
      dropped: ["/tmp/e2e/04-replacements"],
      plan: S3_PLAN,
      replacements: { "髙.png": "高", "﨑.txt": "崎" },
      replacementsConfirmed: true,
      job: "create",
      busy: true,
      progress: STARTING,
    });
    return;
  }
  if (id === "s5") {
    store.applyScene({ mode: "extract" });
    return;
  }
  if (id === "s6" || id === "s7" || id === "s8") {
    const list = id === "s7" ? S7_LIST : S6_LIST;
    installApi(
      mockApi(null, (encoding) => (encoding === "utf8" ? S7_LIST : S6_LIST)),
    );
    store.applyScene({
      mode: "extract",
      dropped: ["/tmp/e2e/cp932-typical.zip"],
      archivePath: "/tmp/e2e/cp932-typical.zip",
      list,
      encodingOverride: list.encoding === "utf8" ? "utf8" : "cp932",
      lastDest: id === "s8" ? E2E_EXTRACT_DEST : null,
      status: id === "s8" ? "解凍しました" : "",
      job: id === "s8" ? "extract" : null,
      progress:
        id === "s8"
          ? {
              phase: "done",
              currentEntry: null,
              entriesDone: 1,
              entriesTotal: 1,
              bytesDone: 4,
              bytesTotal: 4,
            }
          : null,
    });
    return;
  }
  store.applyScene({ mode: "create" });
}

export function installE2eHarness(): E2eApi {
  const api: E2eApi = {
    reset() {
      resetApi();
      useAppStore.getState().setMode("create");
    },
    loadScene,
    getState() {
      const s = useAppStore.getState();
      return {
        mode: s.mode,
        lastDest: s.lastDest,
        sheetOpen: s.sheetOpen,
        encoding: s.list?.encoding ?? s.encodingOverride,
        names: s.mode === "create" ? (s.plan?.entries ?? []) : (s.list?.entries.map((e) => e.displayName) ?? []),
      };
    },
  };
  window.__WINZIP_E2E__ = api;
  const scene = new URLSearchParams(window.location.search).get("scene");
  if (scene) {
    loadScene(scene);
  }
  return api;
}

declare global {
  interface Window {
    __WINZIP_E2E__?: E2eApi;
  }
}
