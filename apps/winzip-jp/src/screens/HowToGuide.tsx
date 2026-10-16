import { UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";

export function HowToGuide({ onDismiss }: { onDismiss: () => void }) {
  return (
    <section className="guide" data-testid="guide" aria-label={COPY.guideTitle}>
      <div className="guide-head">
        <ol className="guide-steps">
          <li>{COPY.guideStepDrop}</li>
          <li>{COPY.guideStepOptions}</li>
          <li>{COPY.guideStepSave}</li>
        </ol>
        <UnstyledButton
          type="button"
          className="cmd-btn cmd-btn-quiet guide-dismiss"
          data-testid="guide-dismiss"
          onClick={onDismiss}
        >
          {COPY.guideDismiss}
        </UnstyledButton>
      </div>
      <p className="guide-extract">{COPY.guideExtract}</p>
    </section>
  );
}
