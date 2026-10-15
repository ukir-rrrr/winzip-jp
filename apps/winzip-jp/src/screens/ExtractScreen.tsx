import { UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import { EmptyWell, ErrorNote } from "./shared";
import type { ListReport, NameEncoding } from "../types";

export function ExtractScreen({
  list,
  error,
  encoding,
  onPick,
  onExtract,
  onReset,
  onEncoding,
}: {
  list: ListReport | null;
  error: string;
  encoding: NameEncoding | null;
  onPick: () => void;
  onExtract: () => void;
  onReset: () => void;
  onEncoding: (encoding: NameEncoding) => void;
}) {
  if (!list) {
    return (
      <>
        <EmptyWell mode="extract" onPick={onPick} />
        <ErrorNote message={error} />
      </>
    );
  }

  const current = encoding ?? list.encoding;
  const emphasize = list.ambiguous || list.confidence < 0.6;

  return (
    <div className="work-pane" data-testid="extract-list">
      <div className={emphasize ? "encoding-block is-warn" : "encoding-block"}>
        <p className="encoding-label" id="encoding-label">
          {COPY.encoding}
        </p>
        <div className="mode-switch encoding-switch" role="radiogroup" aria-labelledby="encoding-label">
          <UnstyledButton
            type="button"
            role="radio"
            aria-checked={current === "cp932"}
            className={current === "cp932" ? "mode-tab is-active" : "mode-tab"}
            data-testid="enc-cp932"
            onClick={() => onEncoding("cp932")}
          >
            {COPY.encodingCp932}
          </UnstyledButton>
          <UnstyledButton
            type="button"
            role="radio"
            aria-checked={current === "utf8"}
            className={current === "utf8" ? "mode-tab is-active" : "mode-tab"}
            data-testid="enc-utf8"
            onClick={() => onEncoding("utf8")}
          >
            {COPY.encodingUtf8}
          </UnstyledButton>
        </div>
        {emphasize ? <p className="encoding-hint">{COPY.encodingAmbiguous}</p> : null}
      </div>
      <ul className="file-list" data-testid="file-list">
        {list.entries.map((entry) => (
          <li key={entry.nameHex}>{entry.displayName}</li>
        ))}
      </ul>
      <div className="cmd-row">
        <UnstyledButton type="button" className="cmd-btn cmd-btn-quiet" onClick={onReset}>
          {COPY.retry}
        </UnstyledButton>
        <UnstyledButton
          type="button"
          className="cmd-btn"
          data-testid="btn-extract"
          onClick={onExtract}
        >
          {COPY.doExtract}
        </UnstyledButton>
      </div>
      <ErrorNote message={error} />
    </div>
  );
}
