import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { WikiPageDetail } from './wiki-page-detail';
import { FakeWikiOptions, GUIDE_CONTENT, SHORT_CONTENT, expectWikiColumns, pageDetail, wikiApplicationConfig, wikiProviders } from '../wiki-story-fixtures';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';

const withData = (options: FakeWikiOptions = {}) => moduleMetadata({ providers: wikiProviders(options) });

type Story = StoryObj<WikiPageDetail>;
type Context = Parameters<NonNullable<Story['play']>>[0];

async function expectColumns({ canvasElement }: Context, aside: boolean): Promise<void> {
  await waitFor(() => {
    if (!canvasElement.querySelector('h1')) throw new Error('page not rendered yet');
  });
  await expectWikiColumns(canvasElement, { aside });
}

async function expectOutline({ canvasElement }: Context, expected: string[]): Promise<void> {
  const links = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLAnchorElement>('fg-wiki-outline a'));
    if (found.length === 0) throw new Error('outline not rendered yet');
    return found;
  });
  await expect(links.map((a) => a.textContent?.trim())).toEqual(expected);
  for (const link of links) {
    const id = decodeURIComponent(new URL(link.href).hash.slice(1));
    await expect(canvasElement.querySelectorAll(`.wiki-page-detail__body [id="${id}"]`).length).toBe(1);
  }
}

const meta: Meta<WikiPageDetail> = {
  title: 'Wiki/WikiPageDetail',
  component: WikiPageDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'r1', slug: 'Guide-de-contribution', path: ['florian', 'ferrisgit'], showHistory: false },
  decorators: [wikiApplicationConfig, inShellContentArea],
};

export default meta;

export const Page: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectOutline(context, ['Guide de contribution', 'Préparer l’environnement', 'Base de données', 'Interface', 'Proposer un changement', 'Conventions', 'Relecture']);
    await expectColumns(context, true);
    await expect(context.canvasElement.querySelectorAll('.gbt-button--primary').length).toBe(0);
  },
};

export const OutlineNavigation: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectOutline(context, ['Guide de contribution', 'Préparer l’environnement', 'Base de données', 'Interface', 'Proposer un changement', 'Conventions', 'Relecture']);
    const link = within(context.canvasElement.querySelector<HTMLElement>('fg-wiki-outline')!).getByRole('link', { name: 'Relecture' });
    await userEvent.click(link);
    await waitFor(() => expect(document.activeElement?.id).toBe('user-content-relecture'));
    await waitFor(() => {
      const top = document.activeElement!.getBoundingClientRect().top;
      expect(top).toBeGreaterThanOrEqual(0);
      expect(top).toBeLessThan(window.innerHeight);
    });
  },
};

export const WithoutHeadings: Story = {
  args: { slug: 'FAQ' },
  decorators: [withData({ detail: pageDetail('FAQ', SHORT_CONTENT) })],
  play: async (context) => {
    await expectColumns(context, true);
    await expect(context.canvasElement.querySelector('fg-wiki-outline')).toBeNull();
    await expect(context.canvasElement.querySelector('.wiki-page-detail__history-panel')).not.toBeNull();
  },
};

export const ReaderView: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await expectColumns(context, true);
    const actions = within(context.canvasElement.querySelector<HTMLElement>('.gbt-page-header__actions')!);
    await expect(actions.getAllByRole('button').map((b) => b.textContent?.trim())).toEqual(['Historique']);
    await expect(context.canvasElement.querySelector('.wiki-nav__new')).toBeNull();
  },
};

export const RevisionsUnavailable: Story = {
  decorators: [withData({ revisions: 'error' })],
  play: async (context) => {
    await expectOutline(context, ['Guide de contribution', 'Préparer l’environnement', 'Base de données', 'Interface', 'Proposer un changement', 'Conventions', 'Relecture']);
    await expect(context.canvasElement.querySelector('.wiki-page-detail__byline')).toBeNull();
    await expect(context.canvasElement.querySelector('.wiki-page-detail__history-panel')).toBeNull();
  },
};

export const LongTitle: Story = {
  args: { slug: 'Politique-de-securite-et-signalement-des-vulnerabilites' },
  decorators: [withData({ detail: pageDetail('Politique de sécurité et signalement des vulnérabilités', GUIDE_CONTENT) })],
  play: async (context) => {
    await expectColumns(context, true);
  },
};

export const DeleteConfirmation: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Supprimer' }));
    const dialog = await within(document.body).findByRole('dialog', { name: 'Supprimer la page' });
    await expect(within(dialog).getByRole('button', { name: 'Supprimer' })).toBeDisabled();
  },
};

export const History: Story = {
  args: { showHistory: true },
  decorators: [withData()],
  play: async (context) => {
    await expectColumns(context, true);
    const { canvasElement } = context;
    const list = canvasElement.querySelector('.wiki-history')!;
    const axis = list.getBoundingClientRect().left + 16;
    for (const avatar of Array.from(canvasElement.querySelectorAll('.wiki-history__avatar'))) {
      const r = avatar.getBoundingClientRect();
      await expect(Math.abs(r.left + r.width / 2 - axis)).toBeLessThan(1.5);
    }
  },
};

export const HistoryWithVersionOpen: Story = {
  args: { showHistory: true },
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const buttons = await canvas.findAllByRole('button', { name: 'Voir cette version' });
    await userEvent.click(buttons[3]);
    await canvas.findByRole('button', { name: 'Masquer cette version' });
    await expect(canvasElement.querySelectorAll('.wiki-history__version').length).toBe(1);
  },
};

export const NotFound: Story = {
  args: { slug: 'Page-supprimee' },
  decorators: [withData({ detail: 'error', revisions: 'error' })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('.wiki-page-detail__not-found')).not.toBeNull());
    await expectWikiColumns(canvasElement, { aside: false });
  },
};

export const Loading: Story = {
  decorators: [withData({ list: 'loading', detail: 'loading', revisions: 'loading' })],
};
