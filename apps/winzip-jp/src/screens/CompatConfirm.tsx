import { useState } from "react";
import { UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import { CompatNotice } from "./CompatNotice";

export function CompatConfirm({
  password,
  split,
  onProceed,
  onWithoutPassword,
}: {
  password: boolean;
  split: boolean;
  onProceed: (skipNext: boolean) => void;
  onWithoutPassword: (skipNext: boolean) => void;
}) {
  const [skipNext, setSkipNext] = useState(false);

  return (
    <div
      className="sheet compat-confirm"
      role="dialog"
      aria-modal="true"
      aria-label={password ? COPY.compatAesAlert : COPY.compatSplitAlert}
      data-testid="compat-confirm"
    >
      <CompatNotice password={password} split={split} />
      <label className="compat-skip">
        <input
          type="checkbox"
          checked={skipNext}
          data-testid="compat-skip"
          onChange={(event) => setSkipNext(event.target.checked)}
        />
        {COPY.compatSkipNext}
      </label>
      <div className="sheet-actions">
        <UnstyledButton
          type="button"
          className="cmd-btn cmd-btn-quiet"
          data-testid="btn-create-without-password"
          onClick={() => onWithoutPassword(skipNext)}
        >
          {COPY.compatWithoutPassword}
        </UnstyledButton>
        <UnstyledButton
          type="button"
          className="cmd-btn"
          data-testid="btn-create-anyway"
          onClick={() => onProceed(skipNext)}
        >
          {COPY.compatProceed}
        </UnstyledButton>
      </div>
    </div>
  );
}
