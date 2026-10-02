import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter } from '@angular/router';
import { expect } from 'storybook/test';
import { PageLayout } from '@masmarino/gabarit';
import { DocsNav } from './docs-nav';
import { DocsService } from '../docs.service';
import { DOCS_INDEX, fakeDocsService } from '../docs-fixtures';
import { atPhoneWidth, inDarkTheme, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';

const withRouter = applicationConfig({ providers: [provideRouter([{ path: '**', children: [] }]), provideLocationMocks(), { provide: DocsService, useValue: fakeDocsService() }] });

// The nav follows the page layout's width (container query), so show it in a page-layout nav column.
const render = (args: Partial<DocsNav>) => ({
  props: args,
  template: `<gbt-page-layout width="wide" navLabel="Documentation"><fg-docs-nav page-nav [index]="index" [section]="section" [page]="page" /><p>Contenu</p></gbt-page-layout>`,
});

async function expectNoOverflow({ canvasElement }: { canvasElement: HTMLElement }) {
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
}

const meta: Meta<DocsNav> = {
  title: 'Docs/DocsNav',
  component: DocsNav,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, withRouter, moduleMetadata({ imports: [PageLayout] }), inShellContentArea],
  args: { index: DOCS_INDEX, section: 'ci-cd', page: 'reference-yaml' },
  render,
};

export default meta;
type Story = StoryObj<DocsNav>;

export const CurrentPage: Story = {
  play: async (context) => {
    await expectNoOverflow(context);
    await expect(context.canvasElement.querySelector('a[aria-current="page"]')?.textContent?.trim()).toBe('Référence de .ferrisgit-ci.yml');
  },
};

export const Dark: Story = {
  decorators: [inDarkTheme],
  play: expectNoOverflow,
};

export const Phone: Story = {
  decorators: [atPhoneWidth],
  play: async (context) => {
    await expectNoOverflow(context);
    await expect(getComputedStyle(context.canvasElement.querySelector('.docs-nav__body')!).display).toBe('none');
  },
};

export const PhoneDark: Story = {
  decorators: [atPhoneWidth, inDarkTheme],
  play: expectNoOverflow,
};
