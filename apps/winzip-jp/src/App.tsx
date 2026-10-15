import { useEffect } from "react";
import { Box, Title, UnstyledButton } from "@mantine/core";
import { hasTauri } from "./api";
import { COPY } from "./copy";
import { CreateScreen } from "./screens/CreateScreen";
import { ExtractScreen } from "./screens/ExtractScreen";
import { ProgressOverlay } from "./screens/ProgressOverlay";
import { UnmappableSheet } from "./screens/UnmappableSheet";
import { useAppStore } from "./store";
import type { ProgressDto } from "./types";
import { SPLIT_PRESET_MB } from "./types";
import "./App.css";

export default function App() {
  const mode = useAppStore((s) => s.mode);
  const plan = useAppStore((s) => s.plan);
  const list = useAppStore((s) => s.list);
  const error = useAppStore((s) => s.error);
  const sheet = useAppStore((s) => s.sheet);
  const issues = plan?.issues ?? [];
  const replacements = useAppStore((s) => s.replacements);
  const job = useAppStore((s) => s.job);
  const confirming = useAppStore((s) => s.confirming);
  const encodingOverride = useAppStore((s) => s.encodingOverride);
  const aesEnabled = useAppStore((s) => s.aesEnabled);
  const password = useAppStore((s) => s.password);
  const splitEnabled = useAppStore((s) => s.splitEnabled);
  const splitPresetMb = useAppStore((s) => s.splitPresetMb);
  const splitCustomMb = useAppStore((s) => s.splitCustomMb);
  const isCreate = mode === "create";
  const modalOpen = sheet === "open" || job !== null;

  useEffect(() => {
    if (!hasTauri()) {
      return;
    }
    let cancelled = false;
    let unlistenProgress: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      if (cancelled) {
        return;
      }
      unlistenProgress = await listen<ProgressDto>("zip://progress", (event) => {
        useAppStore.getState().setProgress(event.payload);
      });
      if (cancelled) {
        unlistenProgress();
        return;
      }
      unlistenDrop = await listen<string[]>("zip://dropped", (event) => {
        void useAppStore.getState().ingestDropped(event.payload);
      });
      if (cancelled) {
        unlistenProgress();
        unlistenDrop();
      }
    })();
    return () => {
      cancelled = true;
      unlistenProgress?.();
      unlistenDrop?.();
    };
  }, []);

  function onDragOver(event: React.DragEvent) {
    event.preventDefault();
  }

  function onDrop(event: React.DragEvent) {
    event.preventDefault();
    if (hasTauri()) {
      return;
    }
    const names = Array.from(event.dataTransfer.files).map((file) => file.name);
    void useAppStore.getState().ingestDropped(names);
  }

  return (
    <Box className="shell" data-ui data-mode={mode} data-testid="app-shell">
      <Box component="header" className="toolbar" inert={modalOpen || undefined}>
        <Title order={1} className="app-title">
          {COPY.appTitle}
        </Title>
        {isCreate ? (
          <div className="opt-switches">
            <UnstyledButton
              type="button"
              role="switch"
              aria-checked={aesEnabled}
              aria-label={COPY.aesOption}
              className={aesEnabled ? "aes-switch is-on" : "aes-switch"}
              data-testid="aes-toggle"
              onClick={() => useAppStore.getState().setAesEnabled(!aesEnabled)}
            >
              {COPY.aesSwitch}
            </UnstyledButton>
            <UnstyledButton
              type="button"
              role="switch"
              aria-checked={splitEnabled}
              aria-label={COPY.splitOption}
              className={splitEnabled ? "split-switch is-on" : "split-switch"}
              data-testid="split-toggle"
              onClick={() => useAppStore.getState().setSplitEnabled(!splitEnabled)}
            >
              {COPY.splitSwitch}
            </UnstyledButton>
          </div>
        ) : null}
        <div className="mode-switch" role="tablist" aria-label={COPY.mode}>
          <UnstyledButton
            type="button"
            role="tab"
            id="tab-create"
            aria-label={COPY.create}
            aria-selected={isCreate}
            aria-controls="content-area"
            className={isCreate ? "mode-tab is-active" : "mode-tab"}
            data-testid="tab-create"
            onClick={() => useAppStore.getState().setMode("create")}
          >
            {COPY.create}
          </UnstyledButton>
          <UnstyledButton
            type="button"
            role="tab"
            id="tab-extract"
            aria-label={COPY.extract}
            aria-selected={!isCreate}
            aria-controls="content-area"
            className={!isCreate ? "mode-tab is-active" : "mode-tab"}
            data-testid="tab-extract"
            onClick={() => useAppStore.getState().setMode("extract")}
          >
            {COPY.extract}
          </UnstyledButton>
        </div>
      </Box>

      <Box
        component="main"
        id="content-area"
        className="content"
        role="tabpanel"
        aria-labelledby={isCreate ? "tab-create" : "tab-extract"}
        data-testid="content"
        onDragOver={onDragOver}
        onDrop={onDrop}
      >
        <section
          className="dropzone-shell"
          data-mode={mode}
          aria-label={isCreate ? COPY.dropCreate : COPY.dropExtract}
        >
          <div className="dropzone-body" inert={modalOpen || undefined}>
            {isCreate ? (
              <CreateScreen
                plan={plan}
                error={error}
                canCreate={sheet !== "open"}
                onPick={() => void useAppStore.getState().pickInputs()}
                onCreate={() => void useAppStore.getState().createZip()}
                onReset={() => useAppStore.getState().resetWork()}
              />
            ) : (
              <ExtractScreen
                list={list}
                error={error}
                encoding={encodingOverride}
                onPick={() => void useAppStore.getState().pickInputs()}
                onExtract={() => void useAppStore.getState().extractZip()}
                onReset={() => useAppStore.getState().resetWork()}
                onEncoding={(encoding) => void useAppStore.getState().setEncoding(encoding)}
              />
            )}
          </div>
          {sheet === "open" && issues.length > 0 ? (
            <UnmappableSheet
              issues={issues}
              replacements={replacements}
              locked={confirming}
              onChange={(path, value) => useAppStore.getState().updateReplacement(path, value)}
              onConfirm={() => void useAppStore.getState().confirmAndCreate()}
              onDismiss={() => useAppStore.getState().dismissSheet()}
            />
          ) : null}
          {isCreate && (aesEnabled || splitEnabled) && sheet !== "open" && !job ? (
            <div className="option-banners">
              {aesEnabled ? (
                <div className="aes-banner" data-testid="aes-panel">
                  <p className="aes-warning" data-testid="aes-warning">
                    {COPY.aesExplorerWarning}
                  </p>
                  <p className="aes-note">{COPY.aesUtf8Note}</p>
                  <label className="aes-password-label" htmlFor="aes-password">
                    {COPY.aesOption}
                    <input
                      id="aes-password"
                      className="sheet-input aes-password"
                      type="password"
                      autoComplete="new-password"
                      value={password}
                      data-testid="aes-password"
                      onChange={(event) => useAppStore.getState().setPassword(event.target.value)}
                    />
                  </label>
                </div>
              ) : null}
              {splitEnabled ? (
                <div className="split-banner" data-testid="split-panel">
                  <p className="aes-warning" data-testid="split-warning">
                    {COPY.splitExplorerWarning}
                  </p>
                  <p className="encoding-label" id="split-size-label">
                    {COPY.splitSize}
                  </p>
                  <div
                    className="mode-switch encoding-switch"
                    role="radiogroup"
                    aria-labelledby="split-size-label"
                  >
                    {SPLIT_PRESET_MB.map((mb) => (
                      <UnstyledButton
                        key={mb}
                        type="button"
                        role="radio"
                        aria-checked={splitPresetMb === mb}
                        className={splitPresetMb === mb ? "mode-tab is-active" : "mode-tab"}
                        data-testid={`split-preset-${mb}`}
                        onClick={() => useAppStore.getState().setSplitPresetMb(mb)}
                      >
                        {mb} {COPY.splitMb}
                      </UnstyledButton>
                    ))}
                    <UnstyledButton
                      type="button"
                      role="radio"
                      aria-checked={splitPresetMb === "custom"}
                      className={splitPresetMb === "custom" ? "mode-tab is-active" : "mode-tab"}
                      data-testid="split-preset-custom"
                      onClick={() => useAppStore.getState().setSplitPresetMb("custom")}
                    >
                      {COPY.splitCustom}
                    </UnstyledButton>
                  </div>
                  {splitPresetMb === "custom" ? (
                    <label className="aes-password-label" htmlFor="split-custom-mb">
                      {COPY.splitCustom}
                      <input
                        id="split-custom-mb"
                        className="sheet-input aes-password"
                        type="number"
                        min={1}
                        step={1}
                        inputMode="numeric"
                        value={splitCustomMb}
                        data-testid="split-custom-mb"
                        onChange={(event) =>
                          useAppStore.getState().setSplitCustomMb(event.target.value)
                        }
                      />
                    </label>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
          {job ? (
            <ProgressOverlay
              progress={job.progress}
              job={job.kind}
              dest={job.dest}
              onCancel={() => void useAppStore.getState().cancel()}
              onClose={() => useAppStore.getState().closeJob()}
            />
          ) : null}
        </section>
      </Box>
    </Box>
  );
}
