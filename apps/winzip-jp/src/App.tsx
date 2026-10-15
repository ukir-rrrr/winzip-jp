import { useState } from "react";
import "./App.css";

type Mode = "create" | "extract";

export default function App() {
  const [mode, setMode] = useState<Mode>("create");
  const isCreate = mode === "create";

  return (
    <div className="shell">
      <header className="toolbar">
        <h1 className="app-title">送りZIP</h1>
        <div
          className="mode-switch"
          role="tablist"
          aria-label="モード"
        >
          <button
            type="button"
            role="tab"
            id="tab-create"
            aria-selected={isCreate}
            aria-controls="content-area"
            className={isCreate ? "mode-tab is-active" : "mode-tab"}
            onClick={() => setMode("create")}
          >
            作成
          </button>
          <button
            type="button"
            role="tab"
            id="tab-extract"
            aria-selected={!isCreate}
            aria-controls="content-area"
            className={!isCreate ? "mode-tab is-active" : "mode-tab"}
            onClick={() => setMode("extract")}
          >
            解凍
          </button>
        </div>
      </header>

      <main id="content-area" className="content" role="tabpanel">
        <section
          className="dropzone"
          data-testid="dropzone"
          aria-label={
            isCreate
              ? "ファイルをドロップして ZIP を作成"
              : "文字化けした ZIP をドロップして解凍"
          }
        >
          <p className="dropzone-title">
            {isCreate
              ? "ファイルをドロップして ZIP を作成"
              : "文字化けした ZIP をドロップして解凍"}
          </p>
          <p className="dropzone-hint">
            {isCreate
              ? "Windows で開ける形式（CP932）で保存します"
              : "文字コードを自動判定します。一覧で確認してから展開できます"}
          </p>
        </section>
      </main>
    </div>
  );
}
