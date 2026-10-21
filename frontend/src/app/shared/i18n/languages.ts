/**
 * The languages the interface is translated into, with the locale their dates and numbers are written in. French is
 * the reference: every other language has exactly its keys.
 */
export const LANGUAGE_LOCALES = {
  fr: 'fr-FR',
  en: 'en-US',
  es: 'es-ES',
  it: 'it-IT',
  de: 'de-DE',
} as const;

export type Language = keyof typeof LANGUAGE_LOCALES;

/** Each language under its own name, so that a reader finds theirs whatever the interface is in. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  fr: 'Français',
  en: 'English',
  es: 'Español',
  it: 'Italiano',
  de: 'Deutsch',
};

export const SUPPORTED_LANGUAGES = Object.keys(LANGUAGE_LOCALES) as Language[];

export const FALLBACK_LANGUAGE: Language = 'fr';

export function isSupported(code: string): code is Language {
  return Object.hasOwn(LANGUAGE_LOCALES, code);
}

/**
 * The first translated language among `preferred` (most wanted first), compared on the language part only: `fr-CA`
 * gives `fr`. The fallback language otherwise.
 */
export function pickLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const code = tag.trim().split(/[-_]/)[0]?.toLowerCase() ?? '';
    if (isSupported(code)) {
      return code;
    }
  }
  return FALLBACK_LANGUAGE;
}

export function detectBrowserLanguage(): Language {
  return pickLanguage(globalThis.navigator?.languages ?? []);
}

const STORAGE_KEY = 'ferrisgit_language';

/** The language chosen in this browser, if it is one the interface still has. */
export function storedLanguage(): Language | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored && isSupported(stored) ? stored : null;
  } catch {
    return null;
  }
}

/** Keeps `language` as this browser's choice, or forgets the choice with `null` so the browser's language applies. */
export function rememberLanguage(language: Language | null): void {
  try {
    if (language) {
      localStorage.setItem(STORAGE_KEY, language);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    // Storage can be off (private window, blocked site data): the choice then lasts for this visit only.
  }
}

/** The language the interface opens in: the one chosen in this browser, else the browser's own. */
export function initialLanguage(): Language {
  return storedLanguage() ?? detectBrowserLanguage();
}
