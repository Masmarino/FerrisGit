import type { Meta, StoryObj } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { of } from 'rxjs';
import { ExplorePage } from './explore-page';
import { atPhoneWidth, inDarkTheme, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { CATALOG, CATALOG_ERRORS, catalogPage, LOADING_CATALOG, withPublicCatalog } from '../public-story-fixtures';

/** Layout checks jsdom can't do: nothing overflows and every card stays inside the column. */
async function expectCatalogLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  const cards = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll('.explore-page__results > li'));
    if (found.length === 0) throw new Error('results not rendered yet');
    return found;
  });
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
  const column = canvasElement.querySelector('fg-explore-page')!.getBoundingClientRect();
  for (const card of cards) {
    const rect = card.getBoundingClientRect();
    await expect(rect.right, 'the card stays inside the column').toBeLessThanOrEqual(column.right + 0.5);
  }
}

const meta: Meta<ExplorePage> = {
  title: 'Public/ExplorePage',
  component: ExplorePage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<ExplorePage>;

export const Popular: Story = {
  decorators: [withPublicCatalog()],
  play: expectCatalogLayout,
};

export const Dark: Story = {
  decorators: [withPublicCatalog(), inDarkTheme],
  play: expectCatalogLayout,
};

export const Phone: Story = {
  decorators: [withPublicCatalog({ search: () => of(catalogPage(CATALOG, 45)) }), atPhoneWidth],
  play: expectCatalogLayout,
};

export const PhoneDark: Story = {
  decorators: [withPublicCatalog({ search: () => of(catalogPage(CATALOG, 45)) }), atPhoneWidth, inDarkTheme],
  play: expectCatalogLayout,
};

export const SearchResultsWithPages: Story = {
  decorators: [withPublicCatalog({ url: '/explore?q=ferris&sort=name&page=2', search: () => of(catalogPage(CATALOG.slice(0, 3), 43, 2)) })],
  play: async (context) => {
    await expectCatalogLayout(context);
    await waitFor(() => expect(context.canvasElement.querySelector('.explore-page__page')?.textContent?.trim()).toBe('Page 2 sur 3'));
  },
};

export const NoResults: Story = {
  decorators: [withPublicCatalog({ url: '/explore?q=introuvable', search: () => of(catalogPage([])) })],
};

export const EmptyInstance: Story = {
  decorators: [withPublicCatalog({ search: () => of(catalogPage([])) })],
};

export const Loading: Story = {
  decorators: [withPublicCatalog({ search: LOADING_CATALOG })],
};

export const RateLimited: Story = {
  decorators: [withPublicCatalog({ search: CATALOG_ERRORS.rateLimited })],
};

export const LoadFailed: Story = {
  decorators: [withPublicCatalog({ search: CATALOG_ERRORS.failed })],
};

export const PublicPagesDisabled: Story = {
  decorators: [withPublicCatalog({ publicPagesEnabled: false })],
};

export const PublicPagesDisabledDark: Story = {
  decorators: [withPublicCatalog({ publicPagesEnabled: false }), inDarkTheme],
};
