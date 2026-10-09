import { Injectable, inject, provideAppInitializer } from '@angular/core';
import { Translation, TranslocoLoader, TranslocoService, provideTransloco } from '@jsverse/transloco';
import { Observable, firstValueFrom, of } from 'rxjs';
import fr from '../public/i18n/fr.json';
import { provideTranslator, setActiveLanguage } from '../src/app/shared/i18n/translator';

/** Storybook has no server to load the language file from, so it is bundled. */
@Injectable()
class BundledTranslocoLoader implements TranslocoLoader {
  getTranslation(): Observable<Translation> {
    return of(fr as Translation);
  }
}

export function provideStorybookTransloco() {
  return [
    provideTransloco({ config: { availableLangs: ['fr'], defaultLang: 'fr', prodMode: false }, loader: BundledTranslocoLoader }),
    provideTranslator(),
    provideAppInitializer(() => {
      setActiveLanguage('fr');
      return firstValueFrom(inject(TranslocoService).load('fr'));
    }),
  ];
}
