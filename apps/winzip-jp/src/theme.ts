import { createTheme } from "@mantine/core";

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
      "#3d96ff",
      "#007aff",
      "#0062cc",
      "#004999",
      "#003166",
      "#001833",
    ],
  },
});
