import { COPY } from "./copy";
import type { ExtractReport, ListReport, PlanDto, WriteReport, ZipApi } from "./types";

export function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function jaErr(fallback: string, err: unknown): Error {
  if (err instanceof Error && err.message) {
    return err;
  }
  return new Error(typeof err === "string" && err ? err : fallback);
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(cmd, args);
}

const tauriApi: ZipApi = {
  async planArchive(paths, destNameHint) {
    try {
      return await invoke<PlanDto>("plan_archive", { paths, destNameHint: destNameHint ?? null });
    } catch (err) {
      throw jaErr(COPY.browserCreate, err);
    }
  },
  async createArchive(req, dest) {
    try {
      return await invoke<WriteReport>("create_archive", { req, dest });
    } catch (err) {
      throw jaErr(COPY.browserCreate, err);
    }
  },
  async listArchive(path, encoding) {
    try {
      return await invoke<ListReport>("list_archive", { path, encoding });
    } catch (err) {
      throw jaErr(COPY.browserList, err);
    }
  },
  async extractArchive(req) {
    try {
      return await invoke<ExtractReport>("extract_archive", { req });
    } catch (err) {
      throw jaErr(COPY.browserExtract, err);
    }
  },
  async cancelJob() {
    if (!hasTauri()) {
      return;
    }
    await invoke("cancel_job");
  },
  async pickFiles() {
    if (!hasTauri()) {
      throw new Error(COPY.noFiles);
    }
    return invoke<string[]>("pick_files");
  },
  async pickSaveZip(defaultName) {
    if (!hasTauri()) {
      throw new Error(COPY.noDest);
    }
    return invoke<string | null>("pick_save_zip", { defaultName });
  },
  async pickExtractDir() {
    if (!hasTauri()) {
      throw new Error(COPY.noExtractDest);
    }
    return invoke<string | null>("pick_extract_dir");
  },
};

let api: ZipApi = tauriApi;

export function getApi(): ZipApi {
  return api;
}

export function installApi(partial: Partial<ZipApi>): void {
  api = { ...api, ...partial };
}

export function resetApi(): void {
  api = tauriApi;
}
