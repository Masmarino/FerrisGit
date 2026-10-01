import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { WikiPageEditor } from './wiki-page-editor';
import { FakeWikiOptions, GUIDE_CONTENT, expectWikiColumns, pageDetail, wikiApplicationConfig, wikiProviders } from '../wiki-story-fixtures';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';

const withData = (options: FakeWikiOptions = {}) => moduleMetadata({ providers: wikiProviders({ role: 'contributor', ...options }) });

type Story = StoryObj<WikiPageEditor>;

async function expectPanes(canvasElement: HTMLElement): Promise<void> {
  const card = canvasElement.querySelector('gbt-list-card')!;
  const source = card.querySelector('.wiki-page-editor__source')!.getBoundingClientRect();
  const preview = card.querySelector('.wiki-page-editor__preview')!.getBoundingClientRect();
  if (card.getBoundingClientRect().width > 768) {
    await expect(source.right).toBeLessThanOrEqual(preview.left + 1);
    await expect(Math.abs(source.top - preview.top)).toBeLessThan(1);
    await expect(Math.abs(source.height - preview.height)).toBeLessThan(1);
  } else {
    await expect(preview.top).toBeGreaterThanOrEqual(source.bottom - 1);
    await expect(Math.abs(source.width - preview.width)).toBeLessThan(1);
  }
}

const meta: Meta<WikiPageEditor> = {
  title: 'Wiki/WikiPageEditor',
  component: WikiPageEditor,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'r1', path: ['florian', 'ferrisgit'] },
  decorators: [wikiApplicationConfig, inShellContentArea],
};

export default meta;

export const CreateMode: Story = {
  args: { slug: null },
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('.wiki-nav__list')).not.toBeNull());
    await expectWikiColumns(canvasElement, { aside: false });
    await expectPanes(canvasElement);
  },
};

export const EditMode: Story = {
  args: { slug: 'Guide-de-contribution' },
  decorators: [withData({ detail: pageDetail('Guide de contribution', GUIDE_CONTENT) })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('.wiki-page-editor__preview h2')).not.toBeNull());
    await expectWikiColumns(canvasElement, { aside: false });
    await expectPanes(canvasElement);
    await expect(canvasElement.querySelectorAll('.gbt-button--primary').length).toBe(1);
  },
};

export const Conflict: Story = {
  args: { slug: 'Guide-de-contribution' },
  decorators: [withData({ detail: pageDetail('Guide de contribution', GUIDE_CONTENT), save: 'conflict' })],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvasElement.querySelector('.wiki-page-editor__preview h2')).not.toBeNull());
    await userEvent.click(canvas.getByRole('button', { name: 'Enregistrer' }));
    await waitFor(() => expect(canvasElement.querySelector('.wiki-page-editor__conflict')).not.toBeNull());
    await expect((canvasElement.querySelector('textarea') as HTMLTextAreaElement).value).toBe(GUIDE_CONTENT);
  },
};
