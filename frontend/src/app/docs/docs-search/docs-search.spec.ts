import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { DocsSearch } from './docs-search';
import { DocsSearchHit } from '../docs-search.service';
import { DocsService } from '../docs.service';
import { fakeDocsService } from '../docs-fixtures';

@Component({ standalone: true, template: '' })
class Blank {}

@Component({ standalone: true, imports: [DocsSearch], template: '<fg-docs-search (opened)="opened.push($event)" />' })
class Host {
  opened: DocsSearchHit[] = [];
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

describe('DocsSearch', () => {
  function setup() {
    const docs = fakeDocsService();
    const page = vi.fn(docs.page);
    TestBed.configureTestingModule({
      providers: [provideRouter([{ path: 'docs/:section/:page', component: Blank }]), { provide: DocsService, useValue: { index: docs.index, page } }],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const input = el.querySelector<HTMLInputElement>('input')!;
    const settle = async () => {
      await wait(200);
      await fixture.whenStable();
      fixture.detectChanges();
    };
    const type = async (value: string) => {
      input.dispatchEvent(new FocusEvent('focus'));
      input.value = value;
      input.dispatchEvent(new Event('input'));
      await settle();
    };
    const key = async (name: string) => {
      input.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }));
      await settle();
    };
    const results = () => Array.from(el.querySelectorAll('[role="option"]'));
    return { fixture, el, input, page, type, key, results, router: TestBed.inject(Router) };
  }

  it('is a labelled combobox', () => {
    const { el, input } = setup();

    expect(input.getAttribute('role')).toBe('combobox');
    expect(text(el.querySelector(`label[for="${input.id}"]`))).toBe('Rechercher dans la documentation');
  });

  it('loads the pages on first focus, not before', async () => {
    const { input, page } = setup();
    expect(page).not.toHaveBeenCalled();

    input.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    await wait(0);

    expect(page).toHaveBeenCalledTimes(5);
  });

  it('lists the matching pages with their section and the text around the match', async () => {
    const { el, type, results } = setup();

    await type('chiffrées');

    expect(results()).toHaveLength(1);
    expect(text(results()[0].querySelector('.docs-search__where'))).toBe('CI/CD');
    expect(text(results()[0].querySelector('.docs-search__title'))).toBe('Référence de .ferrisgit-ci.yml');
    expect(text(results()[0].querySelector('.docs-search__excerpt mark'))).toBe('chiffrées');
    expect(text(el.querySelector('[role="status"]'))).toBe('1 résultat');
  });

  it('names the heading that matched', async () => {
    const { type, results } = setup();

    await type('prédéfinies');

    expect(text(results()[0].querySelector('.docs-search__title'))).toBe('Référence de .ferrisgit-ci.yml › Variables prédéfinies');
  });

  it('says so when nothing matches', async () => {
    const { el, type, results } = setup();

    await type('kubernetes');

    expect(results()).toEqual([]);
    expect(el.textContent).toContain('Aucune page ne correspond.');
  });

  it('opens the result chosen with the keyboard, on its heading, and empties the field', async () => {
    const { fixture, input, type, key, router } = setup();

    await type('cache pipeline');
    await key('ArrowDown');
    await key('Enter');

    expect(router.url).toBe('/docs/ci-cd/reference-yaml#user-content-cache');
    expect(input.value).toBe('');
    expect(fixture.componentInstance.opened.map((hit) => hit.pageTitle)).toEqual(['Référence de .ferrisgit-ci.yml']);
  });

  it('closes the results on Escape', async () => {
    const { type, key, results } = setup();

    await type('variables');
    expect(results().length).toBeGreaterThan(0);
    await key('Escape');

    expect(results()).toEqual([]);
  });
});
