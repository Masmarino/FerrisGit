import { FALLBACK_LANGUAGE, pickLanguage } from './languages';

describe('languages', () => {
  it('picks the first translated language the browser asks for, by its language part', () => {
    expect(pickLanguage(['pt-BR', 'de-DE', 'fr-CA'])).toBe('de');
    expect(pickLanguage(['FR'])).toBe('fr');
    expect(pickLanguage(['en-GB'])).toBe('en');
    expect(pickLanguage(['es_MX'])).toBe('es');
    expect(pickLanguage(['it'])).toBe('it');
  });

  it('falls back when none is translated', () => {
    expect(pickLanguage(['ja', 'zh-TW'])).toBe(FALLBACK_LANGUAGE);
    expect(pickLanguage([])).toBe(FALLBACK_LANGUAGE);
  });
});
