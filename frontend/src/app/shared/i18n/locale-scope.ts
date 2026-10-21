import { Component, LOCALE_ID } from '@angular/core';
import { LANGUAGE_LOCALES } from './languages';
import { activeLanguage } from './translator';

/**
 * Gives the views inside it the locale of the active language. Gabarit's format pipes read LOCALE_ID once, when they
 * are created, so the app rebuilds this scope when the language changes (see `App`): dates and numbers then switch
 * along with the text.
 */
@Component({
  selector: 'fg-locale-scope',
  template: '<ng-content />',
  styles: ':host { display: contents; }',
  providers: [{ provide: LOCALE_ID, useFactory: () => LANGUAGE_LOCALES[activeLanguage()] }],
})
export class LocaleScope {}
