import { useState } from "react";

type ThemeMode = "dark" | "light";

export function useDesktopTheme() {
  const [theme, setTheme] = useState<ThemeMode>("dark");
  const isLightTheme = theme === "light";
  const themeButtonLabel = isLightTheme ? "Switch to dark mode" : "Switch to light mode";

  function toggleTheme() {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }

  return {
    theme,
    isLightTheme,
    themeButtonLabel,
    toggleTheme,
  };
}
