import { Component, input, signal } from '@angular/core';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { MarkdownOutlineEntry, MarkdownView } from './markdown-view';
import { inShellContentArea } from '../layout/page-story-helpers';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';

const meta: Meta<MarkdownView> = {
  title: 'Shared/MarkdownView',
  component: MarkdownView,
  tags: ['autodocs'],
};

export default meta;
type Story = StoryObj<MarkdownView>;

export const Basic: Story = {
  args: {
    content: `# FerrisGit\n\nA self-hosted Git platform.\n\n- Repositories\n- Issues\n- Merge requests\n\n\`\`\`rust\nfn main() {}\n\`\`\`\n`,
  },
};

export const RichFormatting: Story = {
  args: {
    content: `## Release notes\n\n**Bold**, *italic*, and \`inline code\`.\n\n> A blockquote.\n\n[A link](https://example.com)\n`,
  },
};

const GUIDE = `# Guide d’installation

FerrisGit tient dans un seul binaire : une base PostgreSQL et un répertoire pour les dépôts suffisent.

Sommaire : [Prérequis](#prérequis) · [Installation](#installation) · [Variables](#variables-denvironnement)

## Prérequis

- PostgreSQL 16 ou plus récent
- Git 2.40 ou plus récent

### Linux

Installez les paquets de votre distribution.

### macOS

\`brew install postgresql@16 git\`

## Installation

\`\`\`sh
## Ceci est un commentaire de script, pas un titre
./ferrisgit migrate && ./ferrisgit serve
\`\`\`

> ## Titre cité
> Un titre dans une citation n’entre pas dans le plan.

## Installation

Un deuxième titre identique reçoit l’identifiant \`-2\`.

#### Variables d’environnement

\`DATABASE_URL\`, \`FERRISGIT_DATA_DIR\`.
`;

@Component({
  selector: 'fg-markdown-outline-demo',
  imports: [MarkdownView, PageLayout, Panel],
  template: `
    <gbt-page-layout width="default" asideWidth="sm">
      <section class="card">
        <div class="card__header">installation.md</div>
        <div class="card__body">
          <fg-markdown-view [content]="content()" [headingOffset]="headingOffset()" (outline)="outline.set($event)" />
        </div>
      </section>
      <div page-aside>
        <gbt-panel heading="Sur cette page" [headingLevel]="2">
          <ul class="outline">
            @for (entry of outline(); track entry.id) {
              <li class="outline__item" [style.padding-left.rem]="(entry.level - 2) * 0.75">
                <a class="outline__link" [href]="'#' + entry.id">{{ entry.text }}</a>
              </li>
            }
          </ul>
        </gbt-panel>
      </div>
    </gbt-page-layout>
  `,
  styles: `
    .card {
      overflow: hidden;
      border: 1px solid color-mix(in srgb, var(--border-color) 60%, transparent);
      border-radius: 12px;
      background: var(--bg-principal);
    }
    .card__header {
      padding: 0.625rem 1rem;
      border-bottom: 1px solid color-mix(in srgb, var(--border-color) 45%, transparent);
      background: var(--bg-hover);
      color: var(--text-primary);
      font-size: 0.8125rem;
      font-weight: 600;
    }
    .card__body {
      padding: 1rem 1.5rem;
      color: var(--text-primary);
      font-size: 0.9375rem;
      line-height: 1.6;
    }
    .outline {
      display: flex;
      flex-direction: column;
      gap: 0.375rem;
      margin: 0;
      padding: 0;
      list-style: none;
    }
    .outline__link {
      color: var(--text-secondary);
      text-decoration: none;
    }
    .outline__link:hover {
      color: var(--primary);
    }
  `,
})
class MarkdownOutlineDemo {
  content = input.required<string>();
  headingOffset = input(0);
  protected outline = signal<MarkdownOutlineEntry[]>([]);
}

async function expectOutlineLinks({ canvasElement }: { canvasElement: HTMLElement }) {
  const links = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('.outline__link'));
    if (found.length === 0) throw new Error('outline not rendered yet');
    return found;
  });
  await expect(links.map((a) => a.textContent?.trim())).toEqual(['Prérequis', 'Linux', 'macOS', 'Installation', 'Installation', 'Variables d’environnement']);
  const view = canvasElement.querySelector('fg-markdown-view') as HTMLElement;
  for (const link of links) {
    const id = link.getAttribute('href')!.slice(1);
    await expect(view.querySelectorAll(`[id="${id}"]`), `target of ${id}`).toHaveLength(1);
  }
  const ids = Array.from(view.querySelectorAll('[id]')).map((node) => node.id);
  await expect(new Set(ids).size, 'unique ids').toBe(ids.length);
}

type DemoStory = StoryObj<MarkdownOutlineDemo>;

export const WithOutline: DemoStory = {
  decorators: [moduleMetadata({ imports: [MarkdownOutlineDemo] }), inShellContentArea],
  parameters: { layout: 'fullscreen' },
  render: () => ({
    props: { content: GUIDE },
    template: `<fg-markdown-outline-demo [content]="content" />`,
  }),
  play: expectOutlineLinks,
};

export const WithOutlineAndOffset: DemoStory = {
  decorators: [moduleMetadata({ imports: [MarkdownOutlineDemo] }), inShellContentArea],
  parameters: { layout: 'fullscreen' },
  render: () => ({
    props: { content: GUIDE },
    template: `<fg-markdown-outline-demo [content]="content" [headingOffset]="1" />`,
  }),
  play: async ({ canvasElement }) => {
    const links = await waitFor(() => {
      const found = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('.outline__link'));
      if (found.length === 0) throw new Error('outline not rendered yet');
      return found;
    });
    await expect(links.map((a) => a.textContent?.trim())).toEqual(['Guide d’installation', 'Prérequis', 'Linux', 'macOS', 'Installation', 'Installation']);
  },
};

const TASKS = `## Feuille de route

Une liste ordinaire, pour comparer :

- Dépôts et arborescence
- Issues et merge requests

Une liste de tâches serrée :

- [x] Authentification par jeton
- [ ] Webhooks signés
- [ ] Une tâche au libellé assez long pour passer à la ligne sur un écran étroit, et vérifier que le texte revient bien sous le texte, pas sous la case

Une liste de tâches aérée (paragraphes) :

- [x] Migrer la base

- [ ] Réindexer les dépôts

Des tâches imbriquées, sous une liste ordinaire :

- Version 1.2
  - [x] Pipelines
  - [ ] Releases
    - [ ] Notes de version
    - Une puce ordinaire dans une tâche
- Version 1.3

1. Une liste numérotée
2. Reste numérotée
`;

@Component({
  selector: 'fg-markdown-prose-demo',
  imports: [MarkdownView],
  template: `
    <section class="prose-demo">
      <div class="prose-demo__header">roadmap.md</div>
      <div class="prose-demo__body"><fg-markdown-view [content]="content()" /></div>
    </section>
  `,
  styleUrl: './markdown-view.stories.scss',
})
class MarkdownProseDemo {
  content = input.required<string>();
}

function textLeft(li: HTMLElement): number {
  const walker = document.createTreeWalker(li, NodeFilter.SHOW_TEXT, { acceptNode: (node) => (node.textContent?.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP) });
  const text = walker.nextNode()!;
  const first = text.textContent!.length - text.textContent!.trimStart().length;
  const range = document.createRange();
  range.setStart(text, first);
  range.setEnd(text, first + 1);
  return range.getBoundingClientRect().left;
}

export const TaskLists: StoryObj<MarkdownProseDemo> = {
  decorators: [moduleMetadata({ imports: [MarkdownProseDemo] }), inShellContentArea],
  parameters: { layout: 'fullscreen' },
  render: () => ({
    props: { content: TASKS },
    template: `<fg-markdown-prose-demo [content]="content" />`,
  }),
  play: async ({ canvasElement }) => {
    const view = await waitFor(() => {
      const found = canvasElement.querySelector<HTMLElement>('.markdown-view');
      if (!found?.querySelector('input')) throw new Error('markdown not rendered yet');
      return found;
    });
    const items = Array.from(view.querySelectorAll<HTMLLIElement>('ul > li'));
    const isTask = (li: HTMLLIElement) => !!li.querySelector(':scope > input, :scope > p:first-child > input');
    const tasks = items.filter(isTask);
    const plain = items.filter((li) => !isTask(li));
    await expect(tasks, 'task items').toHaveLength(8);
    for (const li of tasks) {
      await expect(getComputedStyle(li).listStyleType, `no bullet: ${li.textContent?.trim().slice(0, 30)}`).toBe('none');
    }
    for (const li of plain) {
      await expect(getComputedStyle(li).listStyleType, `bullet kept: ${li.textContent?.trim().slice(0, 30)}`).not.toBe('none');
    }
    await expect(getComputedStyle(view.querySelector('ol > li')!).listStyleType).toBe('decimal');

    // The checkbox hangs in the gutter, so task text starts where ordinary item text does (within 3px).
    const topPlain = view.querySelector<HTMLLIElement>(':scope > ul > li')!;
    const topTasks = tasks.filter((li) => li.parentElement!.parentElement === view);
    for (const li of topTasks) {
      await expect(Math.abs(textLeft(li) - textLeft(topPlain)), `text aligned: ${li.textContent?.trim().slice(0, 30)}`).toBeLessThanOrEqual(3);
      const box = li.querySelector('input')!.getBoundingClientRect();
      await expect(box.left, 'checkbox inside the card body').toBeGreaterThanOrEqual(view.getBoundingClientRect().left);
    }
  },
};
