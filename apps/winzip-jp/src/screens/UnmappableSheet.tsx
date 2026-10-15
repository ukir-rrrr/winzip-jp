import { Text, UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import { replacementFieldId, replacementRows, suggestedPath } from "../replacements";
import type { Unmappable, UnmappableKind } from "../types";

function kindLabel(kind: UnmappableKind): string {
  if (kind === "compatIdeograph") {
    return COPY.kindCompat;
  }
  if (kind === "emoji") {
    return COPY.kindEmoji;
  }
  return COPY.kindOther;
}

export function UnmappableSheet({
  issues,
  replacements,
  onChange,
  onConfirm,
  onDismiss,
}: {
  issues: Unmappable[];
  replacements: Record<string, string>;
  onChange: (relativePath: string, value: string) => void;
  onConfirm: () => void;
  onDismiss: () => void;
}) {
  return (
    <div
      className="sheet"
      role="dialog"
      aria-modal="true"
      aria-labelledby="unmappable-title"
      data-testid="unmappable-sheet"
    >
      <Text id="unmappable-title" className="sheet-title" data-testid="unmappable-title">
        {COPY.sheetTitle}
      </Text>
      <div className="sheet-table-wrap">
        <table className="sheet-table">
          <thead>
            <tr>
              <th>{COPY.colName}</th>
              <th>{COPY.colChar}</th>
              <th>{COPY.colReplacement}</th>
            </tr>
          </thead>
          <tbody>
            {replacementRows(issues).map((issue) => (
              <tr key={`${issue.relativePath}:${issue.originalChar}`}>
                <td>{issue.relativePath}</td>
                <td>
                  <span className="sheet-char">{issue.originalChar}</span>
                  <span className="sheet-kind">{kindLabel(issue.kind)}</span>
                </td>
                <td>
                  <label className="sr-only" htmlFor={replacementFieldId(issue.relativePath)}>
                    {COPY.colReplacement}
                  </label>
                  <input
                    id={replacementFieldId(issue.relativePath)}
                    className="sheet-input"
                    value={replacements[issue.relativePath] ?? suggestedPath(issue.relativePath, issues)}
                    onChange={(event) => onChange(issue.relativePath, event.target.value)}
                    data-testid={`repl-${issue.relativePath}`}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="sheet-actions">
        <UnstyledButton
          type="button"
          className="cmd-btn cmd-btn-quiet"
          onClick={onDismiss}
        >
          {COPY.dismissSheet}
        </UnstyledButton>
        <UnstyledButton
          type="button"
          className="cmd-btn"
          data-testid="btn-confirm-replace"
          onClick={onConfirm}
        >
          {COPY.confirmReplace}
        </UnstyledButton>
      </div>
    </div>
  );
}
