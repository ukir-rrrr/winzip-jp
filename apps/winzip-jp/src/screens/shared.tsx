import { COPY } from "../copy";
import type { Mode } from "../types";

export function ArchiveMark({ mode }: { mode: Mode }) {
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

export function EmptyWell({
  mode,
  onPick,
}: {
  mode: Mode;
  onPick: () => void;
}) {
  const isCreate = mode === "create";
  return (
    <div className="dropzone" data-testid="dropzone">
      <ArchiveMark mode={mode} />
      <p className="dropzone-title" data-testid="dropzone-title">
        {isCreate ? COPY.dropCreate : COPY.dropExtract}
      </p>
      <p className="dropzone-hint">{isCreate ? COPY.hintCreate : COPY.hintExtract}</p>
      <button
        type="button"
        className="cmd-btn"
        data-testid={isCreate ? "btn-pick-files" : "btn-pick-zip"}
        onClick={onPick}
      >
        {isCreate ? COPY.pickFiles : COPY.pickZip}
      </button>
    </div>
  );
}

export function ErrorNote({ message }: { message: string }) {
  if (!message) {
    return null;
  }
  return (
    <p className="status-text is-error" data-testid="status-text" role="alert">
      {message}
    </p>
  );
}
