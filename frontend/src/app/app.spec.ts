import { Component, LOCALE_ID, inject } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { App } from './app';
import { setActiveLanguage } from './shared/i18n/translator';

@Component({ template: '<p class="locale">{{ locale }}</p>' })
class ShowsLocale {
  readonly locale = inject(LOCALE_ID);
}

describe('App', () => {
  it('rebuilds the page when the language changes, so that it formats in the new locale', async () => {
    TestBed.configureTestingModule({ providers: [provideRouter([{ path: '**', component: ShowsLocale }])] });
    const fixture = TestBed.createComponent(App);
    await TestBed.inject(Router).navigateByUrl('/');
    await fixture.whenStable();
    const shown = () => (fixture.nativeElement as HTMLElement).querySelector('.locale')?.textContent;
    expect(shown()).toBe('fr-FR');

    setActiveLanguage('en');
    await fixture.whenStable();

    expect(shown()).toBe('en-US');
  });
});
