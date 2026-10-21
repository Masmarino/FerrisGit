import { DOCUMENT } from '@angular/common';
import { TestBed } from '@angular/core/testing';
import { LanguageService } from './language.service';
import { storedLanguage } from './languages';
import { activeLanguage } from './translator';

describe('LanguageService', () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('switches to a chosen language and keeps it in this browser, until the choice is forgotten', async () => {
    vi.spyOn(navigator, 'languages', 'get').mockReturnValue(['es-ES']);
    const service = TestBed.inject(LanguageService);

    await service.choose('it');
    expect(activeLanguage()).toBe('it');
    expect(TestBed.inject(DOCUMENT).documentElement.lang).toBe('it');
    expect(storedLanguage()).toBe('it');

    await service.choose(null);
    expect(storedLanguage()).toBeNull();
    expect(activeLanguage()).toBe('es');
  });

  it("takes on the account's language, here too, and leaves the interface alone when it is already in it", async () => {
    const service = TestBed.inject(LanguageService);
    const use = vi.spyOn(service, 'use');

    await service.adopt('de');
    expect(use).toHaveBeenCalledWith('de');
    expect(storedLanguage()).toBe('de');

    await service.adopt('de');
    expect(use).toHaveBeenCalledTimes(1);
  });
});
