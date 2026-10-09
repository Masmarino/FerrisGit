import { ApplicationConfig, LOCALE_ID, inject, isDevMode, provideAppInitializer, provideZonelessChangeDetection } from '@angular/core';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { provideRouter } from '@angular/router';
import { provideTransloco } from '@jsverse/transloco';
import { authInterceptor } from './auth/auth.interceptor';
import { routes } from './app.routes';
import { provideFerrisgitIcons } from './shared/register-icons';
import { LanguageService } from './shared/i18n/language.service';
import { FALLBACK_LANGUAGE, LANGUAGE_LOCALES, SUPPORTED_LANGUAGES, detectBrowserLanguage } from './shared/i18n/languages';
import { TranslocoHttpLoader } from './shared/i18n/transloco-loader';
import { provideTranslator } from './shared/i18n/translator';

export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideRouter(routes),
    provideHttpClient(withInterceptors([authInterceptor])),
    provideFerrisgitIcons(),
    // Gabarit's format pipes write dates, numbers and relative times in this locale.
    { provide: LOCALE_ID, useFactory: () => LANGUAGE_LOCALES[detectBrowserLanguage()] },
    provideTransloco({
      config: {
        availableLangs: SUPPORTED_LANGUAGES,
        defaultLang: FALLBACK_LANGUAGE,
        reRenderOnLangChange: true,
        prodMode: !isDevMode(),
      },
      loader: TranslocoHttpLoader,
    }),
    provideTranslator(),
    // Code calls t() synchronously, so the language has to be loaded before the first component.
    provideAppInitializer(() => inject(LanguageService).use(detectBrowserLanguage())),
  ],
};
