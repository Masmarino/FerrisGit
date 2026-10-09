/**
 * The languages the interface is translated into, with the locale their dates and numbers are written in. French is
 * the reference: every other language has exactly its keys.
 */
export const LANGUAGE_LOCALES = {
  fr: 'fr-FR',
} as const;

export type Language = keyof typeof LANGUAGE_LOCALES;

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
