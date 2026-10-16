import { Alert } from "@mantine/core";
import { COPY } from "../copy";

function WarnMark() {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12.9 3.4c-.4-.7-1.4-.7-1.8 0L2.2 19.1c-.4.7.1 1.6.9 1.6h17.8c.8 0 1.3-.9.9-1.6L12.9 3.4ZM12 9.2c.5 0 .8.4.8.9v4.2c0 .5-.3.9-.8.9s-.8-.4-.8-.9V10.1c0-.5.3-.9.8-.9Zm0 8.3a1.05 1.05 0 1 0 0-2.1 1.05 1.05 0 0 0 0 2.1Z"
      />
    </svg>
  );
}

export function CompatNotice({
  password,
  split,
}: {
  password: boolean;
  split: boolean;
}) {
  if (!password && !split) {
    return null;
  }

  const title = password ? COPY.compatAesAlert : COPY.compatSplitAlert;
  const hint = password ? COPY.compatAesHint : COPY.compatSplitHint;

  return (
    <Alert
      variant="light"
      color="yellow"
      icon={<WarnMark />}
      title={title}
      classNames={{ root: "compat-alert", title: "compat-alert-title", message: "compat-alert-body" }}
      data-testid={password ? "aes-warning" : "split-warning"}
    >
      <p className="compat-hint" data-testid="compat-hint">
        {hint}
      </p>
      {password && split ? (
        <p className="compat-hint" data-testid="split-warning">
          {COPY.compatSplitHint}
        </p>
      ) : null}
    </Alert>
  );
}
