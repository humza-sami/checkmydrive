export type ThemePref = "system" | "light" | "dark";

export function readPref(key: string): string | null {
  try {
    return localStorage.getItem(`checkmydrive.${key}`);
  } catch {
    return null;
  }
}
export function writePref(key: string, value: string) {
  try {
    localStorage.setItem(`checkmydrive.${key}`, value);
  } catch {
    /* storage unavailable: ignore */
  }
}

export function applyTheme(pref: ThemePref) {
  const root = document.documentElement;
  if (pref === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", pref);
}
