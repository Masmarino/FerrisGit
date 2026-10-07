// Storybook fixtures and fakes for the wiki stories, not imported by the app.
import { Provider, signal } from '@angular/core';
import { applicationConfig } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { provideFerrisgitIcons } from '../shared/register-icons';
import { expect } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { WikiList, WikiPageDetail, WikiRevision, WikiService } from './wiki.service';
import { RepositoryContext, RepositoryContextService } from '../repositories/repository-context.service';
import { daysAgo } from '../shared/layout/page-story-helpers';

export type Role = 'owner' | 'reader' | 'contributor' | 'maintainer' | null;

export const PAGES: WikiList = {
  headSha: 'wiki-head',
  pages: [
    { slug: 'Home', title: 'Home' },
    { slug: 'Guide-de-contribution', title: 'Guide de contribution' },
    { slug: 'Architecture', title: 'Architecture' },
    { slug: 'Deploiement-en-production', title: 'Déploiement en production' },
    { slug: 'Runners-CI', title: 'Runners CI' },
    { slug: 'FAQ', title: 'FAQ' },
    { slug: 'Conventions-de-nommage', title: 'Conventions de nommage' },
    { slug: 'Sauvegardes-et-restauration', title: 'Sauvegardes et restauration' },
    { slug: 'Notes-de-version', title: 'Notes de version' },
    { slug: 'Politique-de-securite-et-signalement-des-vulnerabilites', title: 'Politique de sécurité et signalement des vulnérabilités' },
  ],
};

export const GUIDE_CONTENT = `# Guide de contribution

Merci de contribuer à FerrisGit ! Cette page explique comment préparer votre environnement, proposer un changement et le faire relire.

## Préparer l’environnement

Il vous faut Rust stable, Node 22 et une base PostgreSQL locale.

### Base de données

\`\`\`bash
docker compose up -d postgres
export DATABASE_URL=postgres://ferrisgit:ferrisgit@localhost:5432/ferrisgit
\`\`\`

### Interface

Lancez \`npm start\` dans \`frontend/\` : l’interface se recharge à chaque modification.

## Proposer un changement

1. Créez une branche depuis \`main\`
2. Écrivez les tests avant le code
3. Ouvrez une demande de fusion en décrivant **comment tester**

> Une demande de fusion par sujet : les petites relectures vont plus vite.

## Conventions

| Sujet | Règle |
| --- | --- |
| Messages de commit | Impératif, en anglais |
| Branches | \`feature/…\`, \`fix/…\` |

## Relecture

Chaque demande de fusion est relue par un mainteneur. Les pipelines doivent être vertes avant la fusion.
`;

export const SHORT_CONTENT = `Les réponses aux questions qu’on nous pose le plus souvent.

**Puis-je héberger FerrisGit sur un Raspberry Pi ?** Oui, un Pi 4 avec 4 Go de mémoire suffit pour une petite équipe.

**Les dépôts sont-ils compatibles avec Git ?** Oui : ce sont des dépôts Git ordinaires, clonables avec n’importe quel client.`;

export const REVISIONS: WikiRevision[] = [
  { commitSha: '4f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39', authorName: 'alice', authorEmail: 'alice@example.com', committedAt: daysAgo(2), message: 'Documenter la relecture des demandes de fusion' },
  { commitSha: '9b8a7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a07', authorName: 'bastien', authorEmail: 'bastien@example.com', committedAt: daysAgo(6), message: 'Ajouter le tableau des conventions' },
  { commitSha: '1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b', authorName: 'alice', authorEmail: 'alice@example.com', committedAt: daysAgo(13), message: 'Expliquer comment lancer la base de données avec Docker Compose sans toucher à la configuration locale existante' },
  { commitSha: 'c0ffee00d15ea5e0ba5eba11c0ffee00d15ea5e0', authorName: 'florian', authorEmail: 'florian@example.com', committedAt: daysAgo(40), message: 'Update Guide-de-contribution' },
  { commitSha: 'deadbeefcafebabe0123456789abcdef01234567', authorName: 'florian', authorEmail: 'florian@example.com', committedAt: daysAgo(62), message: 'Créer la page' },
];

export const OLD_VERSION = `# Guide de contribution

Première version : ouvrez une demande de fusion vers \`main\` et attendez une relecture.

## Tests

Lancez \`cargo test\` avant de pousser.`;

export function pageDetail(title: string, content: string): WikiPageDetail {
  return { content, headSha: 'page-head', title };
}

export interface FakeWikiOptions {
  role?: Role;
  list?: WikiList | 'loading' | 'error';
  detail?: WikiPageDetail | 'loading' | 'error';
  revisions?: WikiRevision[] | 'loading' | 'error';
  save?: 'ok' | 'conflict';
}

function answer<T>(value: T | 'loading' | 'error'): Observable<T> {
  if (value === 'loading') return NEVER;
  if (value === 'error') return throwError(() => ({ status: 404 }));
  return of(value);
}

export const wikiApplicationConfig = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

export function wikiProviders(options: FakeWikiOptions = {}): Provider[] {
  const detail = options.detail ?? pageDetail('Guide de contribution', GUIDE_CONTENT);
  const fakeWikiService: Pick<WikiService, 'list' | 'detail' | 'revisions' | 'revisionContent' | 'delete' | 'save'> = {
    list: () => answer(options.list ?? PAGES),
    detail: () => answer(detail),
    revisions: () => answer(options.revisions ?? REVISIONS),
    revisionContent: () => of({ content: OLD_VERSION }),
    delete: () => of(undefined),
    save: () => (options.save === 'conflict' ? throwError(() => ({ status: 409 })) : answer(detail)),
  };
  const context: Pick<RepositoryContextService, 'current'> = {
    current: signal<RepositoryContext | null>({ repositoryId: 'r1', path: ['florian', 'ferrisgit'], role: options.role ?? 'maintainer', ancestors: [], groupId: null }),
  };
  return [
    { provide: WikiService, useValue: fakeWikiService },
    { provide: RepositoryContextService, useValue: context },
  ];
}

const rect = (el: Element) => el.getBoundingClientRect();

/** The wiki shell's columns follow `gbt-page-layout`'s width: three beyond 1100px, aside under main beyond 768px, one below that. Nothing overflows and the nav's first line is level with the h1. */
export async function expectWikiColumns(canvasElement: HTMLElement, { aside }: { aside: boolean }): Promise<void> {
  const layout = canvasElement.querySelector('gbt-page-layout')!;
  const width = rect(layout).width;
  const nav = canvasElement.querySelector('.gbt-page-layout__nav')!;
  const main = canvasElement.querySelector('.gbt-page-layout__main')!;
  const asideEl = canvasElement.querySelector('.gbt-page-layout__aside')!;
  await expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(document.documentElement.clientWidth);
  await expect(getComputedStyle(asideEl).display === 'none').toBe(!aside);
  if (width > 768) {
    await expect(rect(nav).right).toBeLessThan(rect(main).left);
    await expect(Math.abs(rect(nav).top - rect(main).top)).toBeLessThan(2);
    await expect(rect(nav).width).toBeCloseTo(240, 0);
    // `@container gbt-page-layout (min-width: 769px)` in wiki-layout.scss: the pages list scrolls on its own.
    // A container name that doesn't match the layout's silently leaves it at `visible`.
    const list = canvasElement.querySelector('.wiki-nav__list');
    if (list) await expect(getComputedStyle(list).overflowY, 'nav list scrolls beside the page (min-width: 769px rule)').toBe('auto');
    const h1 = canvasElement.querySelector('.gbt-page-header__title');
    const heading = canvasElement.querySelector('.wiki-nav__header');
    if (h1 && heading) {
      // The nav's first row is level with the h1's first line (a long title wraps under it).
      const firstLineCentre = rect(h1).top + parseFloat(getComputedStyle(h1).lineHeight) / 2;
      await expect(Math.abs(firstLineCentre - (rect(heading).top + rect(heading).height / 2))).toBeLessThan(3);
    }
    if (aside) {
      if (width > 1100) {
        await expect(rect(main).right).toBeLessThan(rect(asideEl).left);
        await expect(Math.abs(rect(main).top - rect(asideEl).top)).toBeLessThan(2);
      } else {
        await expect(rect(asideEl).top).toBeGreaterThanOrEqual(rect(main).bottom);
        await expect(Math.abs(rect(asideEl).left - rect(main).left)).toBeLessThan(2);
      }
    }
  } else {
    await expect(rect(main).top).toBeGreaterThanOrEqual(rect(nav).bottom);
    await expect(Math.abs(rect(nav).width - rect(main).width)).toBeLessThan(2);
    // Folded: the list is out of the way until the toggle opens it (`@container gbt-page-layout (max-width: 768px)`).
    await expect(getComputedStyle(canvasElement.querySelector('.wiki-nav__body')!).display).toBe('none');
  }
}
