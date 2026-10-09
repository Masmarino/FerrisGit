import { FALLBACK_LANGUAGE, pickLanguage } from './languages';

describe('languages', () => {
  it('picks the first translated language the browser asks for, by its language part', () => {
    expect(pickLanguage(['de-DE', 'fr-CA', 'en'])).toBe('fr');
    expect(pickLanguage(['FR'])).toBe('fr');
  });

  it('falls back when none is translated', () => {
    expect(pickLanguage(['ja', 'zh-TW'])).toBe(FALLBACK_LANGUAGE);
    expect(pickLanguage([])).toBe(FALLBACK_LANGUAGE);
  });
});
