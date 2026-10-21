import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { LanguageService } from '../shared/i18n/language.service';
import { Me, MeService } from './me.service';

const ME: Me = { id: 'u1', username: 'alice', email: 'alice@example.com', isAdmin: false, createdAt: '2026-01-01T00:00:00Z', language: null };

describe('MeService', () => {
  function setup() {
    const languages = { adopt: vi.fn(() => Promise.resolve()), choose: vi.fn(() => Promise.resolve()) };
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), { provide: LanguageService, useValue: languages }],
    });
    return { me: TestBed.inject(MeService), http: TestBed.inject(HttpTestingController), languages };
  }

  it("switches to the account's language once the account is read, and keeps the browser's while it has none", () => {
    const { me, http, languages } = setup();

    me.load();
    http.expectOne('/api/auth/me').flush(ME);
    expect(me.language()).toBeNull();
    expect(languages.adopt).not.toHaveBeenCalled();

    me.load();
    http.expectOne('/api/auth/me').flush({ ...ME, language: 'de' });
    expect(me.language()).toBe('de');
    expect(languages.adopt).toHaveBeenCalledWith('de');
  });

  it('ignores a language the interface no longer has', () => {
    const { me, http, languages } = setup();

    me.load();
    http.expectOne('/api/auth/me').flush({ ...ME, language: 'pt' });

    expect(me.language()).toBeNull();
    expect(languages.adopt).not.toHaveBeenCalled();
  });

  it('saves the language on the account, then switches to it; null goes back to the browser', () => {
    const { me, http, languages } = setup();

    me.updateLanguage('en').subscribe();
    const save = http.expectOne('/api/auth/me/language');
    expect(save.request.method).toBe('PUT');
    expect(save.request.body).toEqual({ language: 'en' });
    expect(languages.adopt).not.toHaveBeenCalled();
    save.flush({ ...ME, language: 'en' });
    expect(languages.adopt).toHaveBeenCalledWith('en');

    me.updateLanguage(null).subscribe();
    const clear = http.expectOne('/api/auth/me/language');
    expect(clear.request.body).toEqual({ language: null });
    clear.flush(ME);
    expect(me.language()).toBeNull();
    expect(languages.choose).toHaveBeenCalledWith(null);
  });
});
