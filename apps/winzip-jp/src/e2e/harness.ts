import { installApi, jaErr, resetApi } from "../api";
import { COPY } from "../copy";
import { COMPAT_SKIP_STORAGE_KEY } from "../compatSkip";
import { GUIDE_STORAGE_KEY } from "../guide";
import { useAppStore } from "../store";
import {
  E2E_CREATE_DEST,
  E2E_EXTRACT_DEST,
  S2_PLAN,
  S3_PLAN,
  S6_LIST,
  S7_LIST,
  STORE_CREATE_PLAN,
  STORE_EXTRACT_LIST,
  STORE_JUNK_PLAN,
} from "./scenes";
import type { ListReport, NameEncoding, PlanDto, WriteReport, ZipApi } from "../types";

function syncGuide(scene: string | null): void {
  localStorage.removeItem(COMPAT_SKIP_STORAGE_KEY);
  if (scene === "guide") {
    localStorage.removeItem(GUIDE_STORAGE_KEY);
    return;
  }
  localStorage.setItem(GUIDE_STORAGE_KEY, "1");
}

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
  resolveCreate = false,
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
      if (resolveCreate) {
        return Promise.resolve<WriteReport>({
          dest,
          entriesWritten: 1,
          bytesWritten: 1,
          excluded: [],
        });
      }
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
  syncGuide(id);
  const store = useAppStore.getState();
  store.setMode("create");
  if (id === "guide") {
    return;
  }

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
    return;
  }
  if (id === "store-create") {
    installApi(mockApi(STORE_CREATE_PLAN, () => null));
    await store.ingestDropped(["/tmp/e2e/store-create"]);
    return;
  }
  if (id === "store-junk") {
    installApi(mockApi(STORE_JUNK_PLAN, () => null));
    await store.ingestDropped(["/tmp/e2e/store-junk"]);
    return;
  }
  if (id === "store-extract") {
    store.setMode("extract");
    installApi(mockApi(null, () => STORE_EXTRACT_LIST));
    await store.ingestDropped(["/tmp/e2e/store-extract.zip"]);
    return;
  }
  if (id === "s9d") {
    installApi(mockApi(S2_PLAN, () => null, true));
    await store.ingestDropped(["/tmp/e2e/03-junk"]);
    return;
  }
  if (id === "crc") {
    store.setMode("extract");
    installApi({
      ...mockApi(null, () => S6_LIST),
      async extractArchive() {
        throw jaErr(COPY.failed, new Error("ZIP が壊れています: a.txt の CRC が一致しません"));
      },
    });
    await store.ingestDropped(["/tmp/e2e/crc.zip"]);
    await store.extractZip();
  }
}

export async function installE2eHarness(): Promise<E2eApi> {
  const scene = new URLSearchParams(window.location.search).get("scene");
  syncGuide(scene);
  const api: E2eApi = {
    reset() {
      resetApi();
      syncGuide(null);
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
