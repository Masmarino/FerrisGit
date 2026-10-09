import { EnvironmentProviders, Injectable, Provider, inject, provideEnvironmentInitializer } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Translation, TranslocoLoader, TranslocoService, provideTransloco } from '@jsverse/transloco';
import { Observable, of } from 'rxjs';
import fr from '../public/i18n/fr.json';
import { provideTranslator, registerTranslator, setActiveLanguage } from './app/shared/i18n/translator';

/**
 * Under this test runner jsdom's `localStorage` is a bare object with no Storage methods, so initializers that read
 * it (AuthService's token signal) throw. Install a small in-memory Storage when that happens.
 */
function createStorage(): Storage {
  const entries = new Map<string, string>()
  return {
    get length(): number {
      return entries.size
    },
    clear: (): void => entries.clear(),
    getItem: (key: string): string | null => entries.get(key) ?? null,
    key: (index: number): string | null => [...entries.keys()][index] ?? null,
    removeItem: (key: string): void => {
      entries.delete(key)
    },
    setItem: (key: string, value: string): void => {
      entries.set(key, String(value))
    },
  } as Storage
}

for (const name of ['localStorage', 'sessionStorage'] as const) {
  const existing = (globalThis as Record<string, unknown>)[name] as Storage | undefined
  if (typeof existing?.getItem !== 'function') {
    Object.defineProperty(globalThis, name, { value: createStorage(), configurable: true })
  }
}

/** Serves the French file synchronously, so that specs render the interface's real text. */
@Injectable()
class InlineTranslocoLoader implements TranslocoLoader {
  getTranslation(): Observable<Translation> {
    return of(fr as Translation);
  }
}

/** Just enough of Transloco's lookup and `{{ param }}` interpolation for specs that build no TestBed. */
function lookup(key: string, params: Record<string, unknown> = {}): string {
  const value = key.split('.').reduce<unknown>((node, part) => (node as Record<string, unknown> | undefined)?.[part], fr);
  if (typeof value !== 'string') {
    return key;
  }
  return value.replace(/\{\{\s*(\w+)\s*\}\}/g, (_, name: string) => String(params[name] ?? ''));
}

// Every spec gets Transloco with the French file. A spec can still provide its own.
const i18nProviders: (Provider | EnvironmentProviders)[] = [
  provideTransloco({ config: { availableLangs: ['fr'], defaultLang: 'fr', prodMode: false }, loader: InlineTranslocoLoader }),
  provideTranslator(),
  provideEnvironmentInitializer(() => {
    inject(TranslocoService).load('fr').subscribe();
  }),
];

// resetTestingModule() drops these providers, so they are applied again after each reset.
const resetTestingModule = TestBed.resetTestingModule.bind(TestBed);
TestBed.resetTestingModule = () => {
  const testBed = resetTestingModule();
  testBed.configureTestingModule({ providers: i18nProviders });
  return testBed;
};

beforeEach(() => {
  // Specs without a TestBed still get French text; those with one use the real service.
  registerTranslator(lookup);
  setActiveLanguage('fr');
  TestBed.configureTestingModule({ providers: i18nProviders });
});
