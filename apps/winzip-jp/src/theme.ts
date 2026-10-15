import { createTheme, type CSSVariablesResolver } from "@mantine/core";

const systemSans =
  '-apple-system, BlinkMacSystemFont, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic UI", sans-serif';

export const theme = createTheme({
  fontFamily: systemSans,
  headings: {
    fontFamily: systemSans,
    fontWeight: "650",
  },
  defaultRadius: "md",
  cursorType: "pointer",
  focusRing: "auto",
  primaryColor: "macos",
  primaryShade: { light: 5, dark: 4 },
  colors: {
    macos: [
      "#e8f3ff",
      "#d0e6ff",
      "#a0cdff",
      "#6eb1ff",
      "#0a84ff",
      "#007aff",
      "#0062cc",
      "#004999",
      "#003166",
      "#001833",
    ],
  },
});

export const cssVariablesResolver: CSSVariablesResolver = (resolved) => ({
  variables: {},
  light: {
    "--app-canvas": "#f2f2f4",
    "--app-toolbar": "#e4e4e8",
    "--app-well-outer": "#d0d0d6",
    "--app-well-inner": "#fafafc",
    "--app-ink": "#1d1d1f",
    "--app-muted": "#6e6e73",
    "--app-accent": resolved.colors.macos[5],
    "--app-track": "#e4e4e8",
    "--app-tab": "#fbfbfd",
  },
  dark: {
    "--app-canvas": "#1c1c1e",
    "--app-toolbar": "#2c2c2e",
    "--app-well-outer": "#48484a",
    "--app-well-inner": "#2c2c2e",
    "--app-ink": "#f5f5f7",
    "--app-muted": "#98989d",
    "--app-accent": resolved.colors.macos[4],
    "--app-track": "#3a3a3c",
    "--app-tab": "#48484a",
  },
});
