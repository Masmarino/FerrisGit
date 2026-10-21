import { Component, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { TranslocoPipe } from '@jsverse/transloco';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { AuthService } from '../../../auth/auth.service';
import { MeService } from '../../../shell/me.service';
import { LanguageService } from '../language.service';
import { LANGUAGE_NAMES, Language, SUPPORTED_LANGUAGES } from '../languages';

/**
 * Switches the interface language where there is no account page to do it: the public pages and the sign-in page. The
 * choice stays in this browser, and on the account too when someone is signed in.
 */
@Component({
  selector: 'fg-language-select',
  imports: [FormsModule, Select, TranslocoPipe],
  template: `<gbt-select [label]="'common.language' | transloco" hideLabel size="sm" [options]="options" [ngModel]="languages.language()" (ngModelChange)="choose($event)" />`,
})
export class LanguageSelect {
  protected readonly languages = inject(LanguageService);
  private readonly auth = inject(AuthService);
  private readonly me = inject(MeService);

  protected readonly options: SelectOption<Language>[] = SUPPORTED_LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language] }));

  protected choose(language: Language): void {
    if (language === this.languages.language()) {
      return;
    }
    void this.languages.choose(language);
    if (this.auth.isAuthenticated()) {
      // The switch above doesn't wait for the account: a failed save only costs the next device this choice.
      this.me.updateLanguage(language).subscribe({ error: () => undefined });
    }
  }
}
