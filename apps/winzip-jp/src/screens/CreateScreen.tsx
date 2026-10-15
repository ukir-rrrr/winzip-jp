import { UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import { EmptyWell, ErrorNote } from "./shared";
import type { PlanDto } from "../types";

export function CreateScreen({
  plan,
  error,
  onPick,
  onCreate,
  onReset,
  canCreate,
}: {
  plan: PlanDto | null;
  error: string;
  onPick: () => void;
  onCreate: () => void;
  onReset: () => void;
  canCreate: boolean;
}) {
  if (!plan) {
    return (
      <>
        <EmptyWell mode="create" onPick={onPick} />
        <ErrorNote message={error} />
      </>
    );
  }

  return (
    <div className="work-pane" data-testid="create-list">
      <p className="work-count">{COPY.fileCount(plan.entries.length)}</p>
      <ul className="file-list" data-testid="file-list">
        {plan.entries.map((name) => (
          <li key={name}>{name}</li>
        ))}
      </ul>
      <div className="cmd-row">
        <UnstyledButton type="button" className="cmd-btn cmd-btn-quiet" onClick={onReset}>
          {COPY.retry}
        </UnstyledButton>
        <UnstyledButton
          type="button"
          className="cmd-btn"
          data-testid="btn-create"
          disabled={!canCreate}
          onClick={onCreate}
        >
          {COPY.makeZip}
        </UnstyledButton>
      </div>
      <ErrorNote message={error} />
    </div>
  );
}
