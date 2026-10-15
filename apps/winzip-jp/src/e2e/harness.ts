import { installApi, resetApi } from "../api";
import { useAppStore } from "../store";
import { E2E_CREATE_DEST, E2E_EXTRACT_DEST, S2_PLAN, S3_PLAN, S6_LIST, S7_LIST } from "./scenes";
import type { ListReport, NameEncoding, PlanDto, WriteReport, ZipApi } from "../types";

export type E2eApi = {
  reset: () => void;
  loadScene: (id: string) => Promise<void>;
  getState: () => {
    mode: string;
    lastDest: string | null;
    sheetOpen: boolean;
    encoding: string | null;
    names: string[];
  };
};

function mockApi(
  plan: PlanDto | null,
  listFor: (encoding: NameEncoding | null) => ListReport | null,
): Partial<ZipApi> {
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
    createArchive(_req, dest) {
      return new Promise<WriteReport>(() => {
        void dest;
      });
    },
    async listArchive(_path, encoding) {
      const list = listFor(encoding);
      if (!list) {
        throw new Error("ZIP が選ばれていません");
      }
      return list;
    },
    async extractArchive(req) {
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

async function loadScene(id: string): Promise<void> {
  resetApi();
  const store = useAppStore.getState();
  store.setMode("create");

  if (id === "s2") {
    installApi(mockApi(S2_PLAN, () => null));
    await store.ingestDropped(["/tmp/e2e/03-junk"]);
    return;
  }
  if (id === "s3") {
    installApi(mockApi(S3_PLAN, () => null));
    await store.ingestDropped(["/tmp/e2e/04-replacements"]);
    return;
  }
  if (id === "s5") {
    store.setMode("extract");
    return;
  }
  if (id === "s6") {
    store.setMode("extract");
    installApi(mockApi(null, (encoding) => (encoding === "utf8" ? S7_LIST : S6_LIST)));
    await store.ingestDropped(["/tmp/e2e/cp932-typical.zip"]);
  }
}

export async function installE2eHarness(): Promise<E2eApi> {
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
        lastDest: s.job?.dest ?? null,
        sheetOpen: s.sheet === "open",
        encoding: s.list?.encoding ?? s.encodingOverride,
        names:
          s.mode === "create"
            ? (s.plan?.entries ?? [])
            : (s.list?.entries.map((entry) => entry.displayName) ?? []),
      };
    },
  };
  window.__WINZIP_E2E__ = api;
  const scene = new URLSearchParams(window.location.search).get("scene");
  if (scene) {
    await loadScene(scene);
  }
  return api;
}

declare global {
  interface Window {
    __WINZIP_E2E__?: E2eApi;
  }
}
