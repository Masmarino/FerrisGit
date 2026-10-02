import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { DocsNav } from './docs-nav';
import { DocsService } from '../docs.service';
import { DOCS_INDEX, fakeDocsService } from '../docs-fixtures';

@Component({ standalone: true, template: '' })
class Blank {}

@Component({ standalone: true, imports: [DocsNav], template: '<fg-docs-nav [index]="index" [section]="section()" [page]="page()" />' })
class Host {
  index = DOCS_INDEX;
  section = signal<string | null>('ci-cd');
  page = signal<string | null>('reference-yaml');
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('DocsNav', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideRouter([{ path: '**', component: Blank }]), { provide: DocsService, useValue: fakeDocsService() }] });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const sectionToggles = () => Array.from(el.querySelectorAll<HTMLButtonElement>('.gbt-disclosure__toggle'));
    const expanded = () => sectionToggles().map((button) => [text(button), button.getAttribute('aria-expanded')]);
    return { fixture, el, sectionToggles, expanded };
  }

  it('lists every section, under a "Documentation" heading, with the search field first', () => {
    const { el, sectionToggles } = setup();

    expect(text(el.querySelector('h2'))).toBe('Documentation');
    expect(el.querySelector('fg-docs-search')).not.toBeNull();
    expect(sectionToggles().map((button) => text(button))).toEqual(['Démarrer', 'CI/CD', 'Administration']);
    expect(Array.from(el.querySelectorAll('h3')).map((heading) => text(heading))).toEqual(['Démarrer', 'CI/CD', 'Administration']);
  });

  it('opens only the current section, and marks the current page', () => {
    const { el, expanded } = setup();

    expect(expanded()).toEqual([
      ['Démarrer', 'false'],
      ['CI/CD', 'true'],
      ['Administration', 'false'],
    ]);
    const current = el.querySelectorAll('a[aria-current="page"]');
    expect(current).toHaveLength(1);
    expect(text(current[0])).toBe('Référence de .ferrisgit-ci.yml');
    expect(current[0].getAttribute('href')).toBe('/docs/ci-cd/reference-yaml');
  });

  it('links every page of the index', () => {
    const { el } = setup();

    expect(Array.from(el.querySelectorAll('a[gbtNavTab]')).map((a) => a.getAttribute('href'))).toEqual([
      '/docs/demarrer/presentation',
      '/docs/demarrer/prise-en-main',
      '/docs/ci-cd/premiers-pas',
      '/docs/ci-cd/reference-yaml',
      '/docs/administration/installation',
    ]);
  });

  it('lets the reader open another section, until they move to another section', () => {
    const { fixture, sectionToggles, expanded } = setup();

    sectionToggles()[0].click();
    fixture.detectChanges();
    expect(expanded()[0]).toEqual(['Démarrer', 'true']);

    fixture.componentInstance.section.set('administration');
    fixture.componentInstance.page.set('installation');
    fixture.detectChanges();
    expect(expanded()).toEqual([
      ['Démarrer', 'false'],
      ['CI/CD', 'false'],
      ['Administration', 'true'],
    ]);
  });

  it('folds behind a toggle on a narrow layout, which says whether it is open', () => {
    const { fixture, el } = setup();
    const toggle = el.querySelector<HTMLButtonElement>('.docs-nav__toggle button')!;
    const body = el.querySelector('.docs-nav__body')!;

    expect(text(toggle)).toBe('Afficher le sommaire');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.getAttribute('aria-controls')).toBe(body.id);

    toggle.click();
    fixture.detectChanges();
    expect(text(toggle)).toBe('Masquer le sommaire');
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(el.querySelector('.docs-nav--open')).not.toBeNull();

    // Choosing a page folds it again.
    el.querySelector<HTMLAnchorElement>('a[gbtNavTab]')!.click();
    fixture.detectChanges();
    expect(el.querySelector('.docs-nav--open')).toBeNull();
  });
});
