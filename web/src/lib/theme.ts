/** The color theme: forced light or dark, or auto to follow the system's `prefers-color-scheme`. */
export type Theme = 'light' | 'auto' | 'dark';

/** Kept in sync with the script in index.html, which applies the theme before the page is drawn. */
const THEME_KEY = 'vugent.theme';

/** A stored theme, or auto for anything else. */
export function parseTheme(value: string | null | undefined): Theme {
  return value === 'light' || value === 'dark' ? value : 'auto';
}

/** The theme the user picked last, or auto. */
export function loadTheme(): Theme {
  try {
    return parseTheme(localStorage.getItem(THEME_KEY));
  } catch {
    // Storage can be blocked; auto is fine then.
    return 'auto';
  }
}

/** Remembers the theme the user picked; auto is the default, so it is not stored. */
export function rememberTheme(theme: Theme) {
  try {
    if (theme === 'auto') {
      localStorage.removeItem(THEME_KEY);
    } else {
      localStorage.setItem(THEME_KEY, theme);
    }
  } catch {
    // Not remembered, but the theme still switches.
  }
}

/** Switches the page's colors (app.css) to the theme through `data-theme` on `<html>`. */
export function applyTheme(theme: Theme) {
  if (theme === 'auto') {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = theme;
  }
}
