import { useState } from "react";
import { Box, Title, UnstyledButton } from "@mantine/core";
import "./App.css";

type Mode = "create" | "extract";

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

export default function App() {
  const [mode, setMode] = useState<Mode>("create");
  const isCreate = mode === "create";

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
        >
          <div
            className="dropzone"
            data-testid="dropzone"
            aria-label={
              isCreate
                ? "ファイルをドロップして ZIP を作成"
                : "文字化けした ZIP をドロップして解凍"
            }
          >
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
          </div>
        </section>
      </Box>
    </Box>
  );
}
