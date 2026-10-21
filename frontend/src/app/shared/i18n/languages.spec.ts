import { FALLBACK_LANGUAGE, initialLanguage, pickLanguage, rememberLanguage, storedLanguage } from './languages';

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

  describe('the choice kept in the browser', () => {
    afterEach(() => {
      localStorage.clear();
      vi.restoreAllMocks();
    });

    it('opens in the remembered language, else in the browser\'s', () => {
      vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['de-DE']);
      expect(storedLanguage()).toBeNull();
      expect(initialLanguage()).toBe('de');

      rememberLanguage('it');
      expect(storedLanguage()).toBe('it');
      expect(initialLanguage()).toBe('it');

      rememberLanguage(null);
      expect(initialLanguage()).toBe('de');
    });

    it('ignores a stored value the interface is not translated into', () => {
      localStorage.setItem('ferrisgit_language', 'pt');
      expect(storedLanguage()).toBeNull();
    });
  });
});
