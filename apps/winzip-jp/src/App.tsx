import { useEffect, useState } from "react";
import { Box, Button, Text, Title, UnstyledButton } from "@mantine/core";
import "./App.css";

type Mode = "create" | "extract";

type ProgressDto = {
  phase: "starting" | "writing" | "finishing" | "done";
  currentEntry: string | null;
  entriesDone: number;
  entriesTotal: number;
  bytesDone: number;
  bytesTotal: number;
};

function ArchiveMark({ mode }: { mode: Mode }) {
  return (
    <svg
      className="dropzone-mark"
      viewBox="0 0 72 72"
      width="72"
      height="72"
      aria-hidden="true"
    >
      <path d="M14 32v22a6 6 0 0 0 6 6h32a6 6 0 0 0 6-6V32" />
      {mode === "create" ? (
        <path d="M36 12v32M24 32l12 12 12-12" />
      ) : (
        <path d="M36 44V12M24 24l12-12 12 12" />
      )}
    </svg>
  );
}

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isS4Draft(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  return new URLSearchParams(window.location.search).get("s4") === "1";
}

const STARTING: ProgressDto = {
  phase: "starting",
  currentEntry: null,
  entriesDone: 0,
  entriesTotal: 0,
  bytesDone: 0,
  bytesTotal: 0,
};

const PHASE_LABEL: Record<ProgressDto["phase"], string> = {
  starting: "開始しています",
  writing: "書き込み中",
  finishing: "仕上げています",
  done: "完了",
};

function progressLabel(progress: ProgressDto | null): string {
  if (!progress) {
    return "進捗はまだありません";
  }
  const phase = PHASE_LABEL[progress.phase];
  if (progress.currentEntry) {
    return `${phase} ${progress.currentEntry}`;
  }
  return phase;
}

async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export default function App() {
  const [mode, setMode] = useState<Mode>("create");
  const [dropped, setDropped] = useState<string[]>([]);
  const [progress, setProgress] = useState<ProgressDto | null>(null);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const isCreate = mode === "create";

  useEffect(() => {
    if (isS4Draft() && !hasTauri()) {
      setProgress(STARTING);
    }
  }, []);

  useEffect(() => {
    if (!hasTauri()) {
      return;
    }
    let cancelled = false;
    let unlistenProgress: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      if (cancelled) {
        return;
      }
      unlistenProgress = await listen<ProgressDto>("zip://progress", (event) => {
        setProgress(event.payload);
      });
      unlistenDrop = await listen<string[]>("zip://dropped", (event) => {
        setDropped(event.payload);
        setStatus(`${event.payload.length} 件を受け取りました`);
      });
    })().catch(() => {
      // プレビューでは Tauri が無い
    });
    return () => {
      cancelled = true;
      unlistenProgress?.();
      unlistenDrop?.();
    };
  }, []);

  async function onCreate() {
    if (!hasTauri()) {
      if (isS4Draft()) {
        setProgress(STARTING);
        setStatus("開始しています");
      } else {
        setStatus("この画面では作成できません");
      }
      return;
    }
    setBusy(true);
    setStatus("");
    try {
      let paths = dropped;
      if (paths.length === 0) {
        paths = await invokeCommand<string[]>("pick_files");
        setDropped(paths);
      }
      if (paths.length === 0) {
        setStatus("ファイルが選ばれていません");
        return;
      }
      const dest = await invokeCommand<string | null>("pick_save_zip", {
        defaultName: "archive.zip",
      });
      if (!dest) {
        setStatus("保存先が選ばれていません");
        return;
      }
      await invokeCommand("create_archive", {
        req: { paths, replacements: {} },
        dest,
      });
      setStatus("作成しました");
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onList() {
    if (!hasTauri()) {
      setStatus("この画面では一覧できません");
      return;
    }
    setBusy(true);
    try {
      let path = dropped.find((p) => p.toLowerCase().endsWith(".zip"));
      if (!path) {
        const files = await invokeCommand<string[]>("pick_files");
        path = files.find((p) => p.toLowerCase().endsWith(".zip")) ?? files[0];
      }
      if (!path) {
        setStatus("ZIP が選ばれていません");
        return;
      }
      const report = await invokeCommand<{
        entries: { displayName: string }[];
        encoding: string;
      }>("list_archive", { path, encoding: null });
      const names = report.entries.map((e) => e.displayName).join("、");
      setStatus(names || "エントリがありません");
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onExtract() {
    if (!hasTauri()) {
      setStatus("この画面では解凍できません");
      return;
    }
    setBusy(true);
    try {
      let archive = dropped.find((p) => p.toLowerCase().endsWith(".zip"));
      if (!archive) {
        const files = await invokeCommand<string[]>("pick_files");
        archive = files.find((p) => p.toLowerCase().endsWith(".zip")) ?? files[0];
      }
      if (!archive) {
        setStatus("ZIP が選ばれていません");
        return;
      }
      const dest = await invokeCommand<string | null>("pick_extract_dir");
      if (!dest) {
        setStatus("展開先が選ばれていません");
        return;
      }
      await invokeCommand("extract_archive", {
        req: { archive, dest, encoding: null, overwrite: false },
      });
      setStatus("解凍しました");
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Box className="shell" data-ui>
      <Box component="header" className="toolbar">
        <Title order={1} className="app-title">
          送りZIP
        </Title>
        <div className="mode-switch" role="tablist" aria-label="モード">
          <UnstyledButton
            type="button"
            role="tab"
            id="tab-create"
            aria-selected={isCreate}
            aria-controls="content-area"
            className={isCreate ? "mode-tab is-active" : "mode-tab"}
            onClick={() => setMode("create")}
          >
            作成
          </UnstyledButton>
          <UnstyledButton
            type="button"
            role="tab"
            id="tab-extract"
            aria-selected={!isCreate}
            aria-controls="content-area"
            className={!isCreate ? "mode-tab is-active" : "mode-tab"}
            onClick={() => setMode("extract")}
          >
            解凍
          </UnstyledButton>
        </div>
      </Box>

      <Box
        component="main"
        id="content-area"
        className="content"
        role="tabpanel"
        aria-labelledby={isCreate ? "tab-create" : "tab-extract"}
      >
        <section
          className="dropzone-shell"
          data-mode={mode}
          aria-label={
            isCreate
              ? "ファイルをドロップして ZIP を作成"
              : "文字化けした ZIP をドロップして解凍"
          }
        >
          <div className="dropzone" data-testid="dropzone">
            <ArchiveMark mode={mode} />
            <p className="dropzone-title">
              {isCreate
                ? "ファイルをドロップして ZIP を作成"
                : "文字化けした ZIP をドロップして解凍"}
            </p>
            <p className="dropzone-hint">
              {isCreate ? (
                "Windows で開ける形式（CP932）で保存します"
              ) : (
                <>
                  文字コードを自動判定します。
                  <br />
                  一覧で確認してから展開できます
                </>
              )}
            </p>
            <div className="cmd-row">
              <Button
                type="button"
                className="cmd-btn"
                data-testid="btn-create"
                variant="filled"
                color="macos"
                disabled={busy}
                onClick={() => void onCreate()}
              >
                作成
              </Button>
              <Button
                type="button"
                className="cmd-btn cmd-btn-quiet"
                data-testid="btn-list"
                variant="subtle"
                color="macos"
                disabled={busy}
                onClick={() => void onList()}
              >
                一覧
              </Button>
              <Button
                type="button"
                className="cmd-btn cmd-btn-quiet"
                data-testid="btn-extract"
                variant="subtle"
                color="macos"
                disabled={busy}
                onClick={() => void onExtract()}
              >
                解凍
              </Button>
            </div>
            <Text className="progress-text" data-testid="progress-text">
              {progressLabel(progress)}
            </Text>
            {status ? (
              <Text className="status-text" data-testid="status-text">
                {status}
              </Text>
            ) : null}
          </div>
        </section>
      </Box>
    </Box>
  );
}
