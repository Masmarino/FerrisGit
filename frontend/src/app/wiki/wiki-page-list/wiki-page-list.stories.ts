import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { WikiPageList } from './wiki-page-list';
import { FakeWikiOptions, PAGES, expectWikiColumns, wikiApplicationConfig, wikiProviders } from '../wiki-story-fixtures';
import { atPhoneWidth, inShellContentArea } from '../../shared/layout/page-story-helpers';

const withData = (options: FakeWikiOptions = {}) => moduleMetadata({ providers: wikiProviders({ role: 'contributor', ...options }) });

type Story = StoryObj<WikiPageList>;

async function rendered(canvasElement: HTMLElement): Promise<void> {
  await waitFor(() => {
    if (canvasElement.querySelector('[aria-busy="true"]')) throw new Error('still loading');
  });
}

const meta: Meta<WikiPageList> = {
  title: 'Wiki/WikiPageList',
  component: WikiPageList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'r1', path: ['florian', 'ferrisgit'] },
  decorators: [wikiApplicationConfig, inShellContentArea],
};

export default meta;

export const Populated: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement);
    await expectWikiColumns(canvasElement, { aside: false });
    await expect(canvasElement.querySelectorAll('.wiki-page-list__items > li').length).toBe(PAGES.pages.length);
  },
};

/** Phone width: the pages nav has to fold above the list via `@container gbt-page-layout (max-width: 768px)`. Rename the container and the nav stays open, failing this check. */
export const PhoneWidth: Story = {
  decorators: [withData(), atPhoneWidth],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement);
    await expect(canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect().width).toBeLessThan(769);
    await expectWikiColumns(canvasElement, { aside: false });
  },
};

export const Search: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement);
    const toggle = canvasElement.querySelector<HTMLElement>('.wiki-nav__toggle button')!;
    if (getComputedStyle(canvasElement.querySelector('.wiki-nav__toggle')!).display !== 'none') {
      await userEvent.click(toggle);
    }
    await userEvent.type(within(canvasElement).getByLabelText('Rechercher une page'), 'deploie');
    await waitFor(() => expect(canvasElement.querySelectorAll('.wiki-nav__list li').length).toBe(1));
  },
};

export const ReaderView: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement);
    await expect(canvasElement.querySelector('.wiki-nav__new')).toBeNull();
  },
};

export const Empty: Story = {
  decorators: [withData({ list: { headSha: null, pages: [] } })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement);
    await expect(canvasElement.querySelectorAll('.gbt-button--primary').length).toBe(1);
  },
};

export const EmptyReaderView: Story = {
  decorators: [withData({ role: 'reader', list: { headSha: null, pages: [] } })],
};

export const Loading: Story = {
  decorators: [withData({ list: 'loading' })],
};

export const LoadError: Story = {
  decorators: [withData({ list: 'error' })],
};
