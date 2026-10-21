import { DOCUMENT } from '@angular/common';
import { Injectable, inject } from '@angular/core';
import { TranslocoService } from '@jsverse/transloco';
import { firstValueFrom } from 'rxjs';
import { Language, detectBrowserLanguage, rememberLanguage, storedLanguage } from './languages';
import { activeLanguage, setActiveLanguage } from './translator';

/** Switches the interface to a language once its file is loaded, so that no text shows as a bare key. */
@Injectable({ providedIn: 'root' })
export class LanguageService {
  private readonly transloco = inject(TranslocoService);
  private readonly document = inject(DOCUMENT);

  readonly language = activeLanguage;

  async use(language: Language): Promise<void> {
    await firstValueFrom(this.transloco.load(language), { defaultValue: undefined });
    this.transloco.setActiveLang(language);
    this.document.documentElement.lang = language;
    setActiveLanguage(language);
  }

  /** Keeps the choice in this browser and switches to it. `null` forgets the choice: the browser's language applies. */
  choose(language: Language | null): Promise<void> {
    rememberLanguage(language);
    return this.use(language ?? detectBrowserLanguage());
  }

  /**
   * Takes on the language saved on the account, in this browser too, so that the next visit opens in it before the
   * account is read. Nothing changes when it is already the one in use.
   */
  async adopt(language: Language): Promise<void> {
    if (storedLanguage() !== language) {
      rememberLanguage(language);
    }
    if (activeLanguage() !== language) {
      await this.use(language);
    }
  }
}
