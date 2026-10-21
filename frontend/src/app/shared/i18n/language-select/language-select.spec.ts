import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { AuthService } from '../../../auth/auth.service';
import { MeService } from '../../../shell/me.service';
import { LanguageService } from '../language.service';
import { LanguageSelect } from './language-select';

describe('LanguageSelect', () => {
  async function setup(signedIn: boolean) {
    const languages = { language: signal('fr'), choose: vi.fn(() => Promise.resolve()) };
    const me = { updateLanguage: vi.fn(() => of({})) };
    TestBed.configureTestingModule({
      imports: [LanguageSelect],
      providers: [
        { provide: LanguageService, useValue: languages },
        { provide: AuthService, useValue: { isAuthenticated: signal(signedIn) } },
        { provide: MeService, useValue: me },
      ],
    });
    const fixture = TestBed.createComponent(LanguageSelect);
    fixture.detectChanges();
    await fixture.whenStable(); // ngModel writes the select's value asynchronously
    fixture.detectChanges();
    return { fixture, languages, me };
  }

  function pick(fixture: { detectChanges(): void; nativeElement: HTMLElement }, label: string): void {
    fixture.nativeElement.querySelector<HTMLButtonElement>('.gbt-select__trigger')!.click();
    fixture.detectChanges();
    Array.from(document.querySelectorAll<HTMLElement>('.gbt-select__option'))
      .find((option) => option.textContent?.trim() === label)!
      .click();
    fixture.detectChanges();
  }

  it('lists each language under its own name and shows the one in use', async () => {
    const { fixture } = await setup(false);

    expect(fixture.nativeElement.querySelector('.gbt-select__trigger').textContent.trim()).toBe('Français');
    fixture.nativeElement.querySelector('.gbt-select__trigger').click();
    fixture.detectChanges();
    const names = Array.from(document.querySelectorAll('.gbt-select__option')).map((option) => option.textContent?.trim());
    expect(names).toEqual(['Français', 'English', 'Español', 'Italiano', 'Deutsch']);
  });

  it('keeps a visitor\'s choice in the browser only', async () => {
    const { fixture, languages, me } = await setup(false);

    pick(fixture, 'Deutsch');

    expect(languages.choose).toHaveBeenCalledWith('de');
    expect(me.updateLanguage).not.toHaveBeenCalled();
  });

  it('saves a signed-in reader\'s choice on the account too', async () => {
    const { fixture, languages, me } = await setup(true);

    pick(fixture, 'English');

    expect(languages.choose).toHaveBeenCalledWith('en');
    expect(me.updateLanguage).toHaveBeenCalledWith('en');
  });
});
