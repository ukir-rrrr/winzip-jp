import { Text, UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import type { JobKind } from "../store";
import type { ProgressDto } from "../types";

const PHASE = {
  starting: COPY.starting,
  writing: COPY.writing,
  finishing: COPY.finishing,
  done: COPY.done,
} as const;

export function ProgressOverlay({
  progress,
  job,
  dest,
  usedPassword = false,
  usedSplit = false,
  onCancel,
  onClose,
}: {
  progress: ProgressDto;
  job: JobKind;
  dest: string | null;
  usedPassword?: boolean;
  usedSplit?: boolean;
  onCancel: () => void;
  onClose: () => void;
}) {
  const done = progress.phase === "done";
  const ratio =
    progress.entriesTotal > 0 ? progress.entriesDone / progress.entriesTotal : done ? 1 : 0.08;
  const title = done ? (job === "extract" ? COPY.extracted : COPY.created) : PHASE[progress.phase];

  return (
    <div
      className="progress-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={COPY.progress}
      data-testid="progress-overlay"
    >
      <Text className="progress-title" data-testid="progress-text">
        {title}
        {progress.currentEntry ? ` ${progress.currentEntry}` : ""}
      </Text>
      <div
        className="progress-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(ratio * 100)}
      >
        <span style={{ width: `${Math.max(8, Math.min(100, ratio * 100))}%` }} />
      </div>
      {dest ? (
        <p className="progress-dest" data-testid={job === "extract" ? "extract-dest" : "create-dest"}>
          {dest}
        </p>
      ) : null}
      {done && (usedPassword || usedSplit) ? (
        <p className="compat-reminder" data-testid="compat-reminder">
          {usedPassword ? COPY.compatCreatedReminder : COPY.compatSplitCreatedReminder}
        </p>
      ) : null}
      {done ? (
        <UnstyledButton type="button" className="cmd-btn" onClick={onClose}>
          {COPY.close}
        </UnstyledButton>
      ) : (
        <UnstyledButton
          type="button"
          className="cmd-btn cmd-btn-quiet"
          aria-label={COPY.cancel}
          data-testid="btn-cancel"
          onClick={onCancel}
        >
          {COPY.cancel}
        </UnstyledButton>
      )}
    </div>
  );
}
