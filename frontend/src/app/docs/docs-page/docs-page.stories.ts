import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { atPhoneWidth, inDarkTheme, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { withDocs } from '../docs-story-fixtures';
import { FAILING_DOCS, fakeDocsService, LOADING_DOCS } from '../docs-fixtures';

// `DocsPage` reads its section and page from the route, so the stories render it through the docs routes.
@Component({ selector: 'fg-docs-route', standalone: true, imports: [RouterOutlet], template: '<router-outlet />' })
class DocsRoute {}

/** Real-layout checks jsdom cannot make: nothing overflows the frame, the article stays inside it. */
async function expectDocsLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  const article = await waitFor(() => {
    const found = canvasElement.querySelector('.docs-page__article');
    if (!found) throw new Error('page not rendered yet');
    return found;
  });
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
  const frame = canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect();
  await expect(article.getBoundingClientRect().right, 'the article stays inside the layout').toBeLessThanOrEqual(frame.right + 0.5);
}

const meta: Meta<DocsRoute> = {
  title: 'Docs/DocsPage',
  component: DocsRoute,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<DocsRoute>;

export const Reference: Story = {
  decorators: [withDocs()],
  play: async (context) => {
    await expectDocsLayout(context);
    await waitFor(() => expect(context.canvasElement.querySelector('aside .wiki-outline__list')).not.toBeNull());
  },
};

export const Dark: Story = {
  decorators: [withDocs(), inDarkTheme],
  play: expectDocsLayout,
};

export const Phone: Story = {
  decorators: [withDocs(), atPhoneWidth],
  play: async (context) => {
    await expectDocsLayout(context);
    const toggle = context.canvasElement.querySelector('.docs-nav__toggle')!;
    await expect(getComputedStyle(toggle).display, 'the sections fold behind a toggle').not.toBe('none');
    await expect(getComputedStyle(context.canvasElement.querySelector('.docs-nav__body')!).display).toBe('none');
    await expect(getComputedStyle(context.canvasElement.querySelector('.gbt-page-layout__aside')!).display, 'no outline on a phone').toBe('none');
  },
};

export const PhoneDark: Story = {
  decorators: [withDocs(), atPhoneWidth, inDarkTheme],
  play: expectDocsLayout,
};

export const FirstPage: Story = {
  decorators: [withDocs({ url: '/docs' })],
  play: async (context) => {
    await expectDocsLayout(context);
    await expect(context.canvasElement.querySelector('article h1')?.textContent?.trim()).toBe('Présentation');
  },
};

export const NotFound: Story = {
  decorators: [withDocs({ url: '/docs/ci-cd/disparue' })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('h1')?.textContent?.trim()).toBe('Page introuvable'));
  },
};

export const NotFoundPhone: Story = {
  decorators: [withDocs({ url: '/docs/ci-cd/disparue' }), atPhoneWidth],
};

export const Loading: Story = {
  decorators: [withDocs({ docs: fakeDocsService({ page: LOADING_DOCS }) })],
};

export const Failed: Story = {
  decorators: [withDocs({ docs: fakeDocsService({ index: FAILING_DOCS }) })],
};
