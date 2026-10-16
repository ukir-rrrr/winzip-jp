import { useState } from "react";
import { UnstyledButton } from "@mantine/core";
import { COPY } from "../copy";
import { useAppStore } from "../store";
import { SPLIT_PRESET_MB } from "../types";
import { CompatNotice } from "./CompatNotice";

export function OptionPanels({
  aesEnabled,
  password,
  splitEnabled,
  splitPresetMb,
  splitCustomMb,
  showAlert,
}: {
  aesEnabled: boolean;
  password: string;
  splitEnabled: boolean;
  splitPresetMb: 8 | 10 | 25 | 100 | "custom";
  splitCustomMb: string;
  showAlert: boolean;
}) {
  const [passwordVisible, setPasswordVisible] = useState(false);
  const both = aesEnabled && splitEnabled;

  return (
    <div className="option-stack" data-testid="option-banners">
      {showAlert ? <CompatNotice password={aesEnabled} split={splitEnabled} /> : null}
      <div className={both ? "option-banners is-both" : "option-banners"}>
        {aesEnabled ? (
          <div className="aes-banner" data-testid="aes-panel">
            <p className="aes-note">{COPY.aesUtf8Note}</p>
            <div className="aes-password-block">
              <label className="aes-password-label" htmlFor="aes-password">
                {COPY.aesOption}
              </label>
              <div className="aes-password-row">
                <input
                  id="aes-password"
                  className="aes-password"
                  type={passwordVisible ? "text" : "password"}
                  autoComplete="new-password"
                  placeholder={COPY.aesPlaceholder}
                  value={password}
                  data-testid="aes-password"
                  onChange={(event) => useAppStore.getState().setPassword(event.target.value)}
                />
                <UnstyledButton
                  type="button"
                  className={passwordVisible ? "aes-reveal is-on" : "aes-reveal"}
                  data-testid="aes-password-toggle"
                  aria-controls="aes-password"
                  aria-pressed={passwordVisible}
                  aria-label={COPY.showPasswordLabel}
                  onClick={() => setPasswordVisible((open) => !open)}
                >
                  {COPY.showPassword}
                </UnstyledButton>
              </div>
            </div>
          </div>
        ) : null}
        {splitEnabled ? (
          <div className="split-banner" data-testid="split-panel">
            <p className="encoding-label" id="split-size-label">
              {COPY.splitSize}
            </p>
            <div
              className="mode-switch encoding-switch split-presets"
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
                  className="aes-password"
                  type="number"
                  min={1}
                  step={1}
                  inputMode="numeric"
                  value={splitCustomMb}
                  data-testid="split-custom-mb"
                  onChange={(event) => useAppStore.getState().setSplitCustomMb(event.target.value)}
                />
              </label>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
