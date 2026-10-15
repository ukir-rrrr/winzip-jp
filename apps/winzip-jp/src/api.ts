import { COPY } from "./copy";
import type { ExtractReport, ListReport, PlanDto, WriteReport, ZipApi } from "./types";

export function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const COPY_MESSAGES: string[] = Object.values(COPY).filter(
  (value): boolean => typeof value === "string",
) as string[];

/** zipcore / コマンド層の日本語エラー。動的部分（パス）の前までを見る。 */
const KNOWN_JA_PREFIXES = [
  "追加できるファイルがありません",
  "キャンセルされました",
  "パスワード付き ZIP はこのバージョンではまだ扱えません",
  "分割 ZIP はこのバージョンではまだ作れません",
  "ZIP64 が必要なため作成できません:",
  "Windows で使えない文字の置き換えを確定してください",
  "フォルダではありません:",
  "ファイルではありません:",
  "エントリ名が長すぎます:",
  "入力が見つかりません:",
  "ファイルでもフォルダでもありません:",
  "同名エントリの連番が上限を超えました:",
  "出力先が入力フォルダの中にあります:",
  "ZIP が壊れています:",
  "ZIP 内のパスが展開先の外に出ます:",
  "次のファイルは既にあります。上書きを許可してください:",
  "パスワード付きのエントリがあります:",
  "展開できないエントリです",
  "ZIP が大きすぎます:",
  "入出力エラー:",
  "内部状態をロックできません",
  "見つかりません",
  "権限がありません",
  "読み書きに失敗しました",
  "保存先へ移せませんでした:",
  "シンボリックリンクは扱えません:",
  "スコープ外のパスです:",
  "ファイル名の UTF-8 フラグ（EFS）が立っています",
  "展開先がフォルダではありません",
  "パスが不正です",
  "この場所にはアクセスできません:",
  "選んだ場所をパスにできません",
  "別の処理が実行中です",
  "作成処理を開始できませんでした",
  "解凍処理を開始できませんでした",
  "保存先がフォルダです",
] as const;

export function isKnownJaMessage(msg: string): boolean {
  if (COPY_MESSAGES.includes(msg)) {
    return true;
  }
  return KNOWN_JA_PREFIXES.some((prefix) => msg === prefix || msg.startsWith(prefix));
}

export function jaErr(fallback: string, err: unknown): Error {
  const msg = err instanceof Error ? err.message : typeof err === "string" ? err : "";
  if (msg && isKnownJaMessage(msg)) {
    return new Error(msg);
  }
  return new Error(fallback);
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(cmd, args);
}

const tauriApi: ZipApi = {
  async planArchive(paths, destNameHint) {
    if (!hasTauri()) {
      throw new Error(COPY.browserCreate);
    }
    try {
      return await invoke<PlanDto>("plan_archive", { paths, destNameHint: destNameHint ?? null });
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
  },
  async createArchive(req, dest) {
    if (!hasTauri()) {
      throw new Error(COPY.browserCreate);
    }
    try {
      return await invoke<WriteReport>("create_archive", { req, dest });
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
  },
  async listArchive(path, encoding) {
    if (!hasTauri()) {
      throw new Error(COPY.browserList);
    }
    try {
      return await invoke<ListReport>("list_archive", { path, encoding });
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
  },
  async extractArchive(req) {
    if (!hasTauri()) {
      throw new Error(COPY.browserExtract);
    }
    try {
      return await invoke<ExtractReport>("extract_archive", { req });
    } catch (err) {
      throw jaErr(COPY.failed, err);
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
    try {
      return await invoke<string[]>("pick_files");
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
  },
  async pickSaveZip(defaultName) {
    if (!hasTauri()) {
      throw new Error(COPY.noDest);
    }
    try {
      return await invoke<string | null>("pick_save_zip", { defaultName });
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
  },
  async pickExtractDir() {
    if (!hasTauri()) {
      throw new Error(COPY.noExtractDest);
    }
    try {
      return await invoke<string | null>("pick_extract_dir");
    } catch (err) {
      throw jaErr(COPY.failed, err);
    }
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
