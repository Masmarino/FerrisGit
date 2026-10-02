import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter } from '@angular/router';
import { expect, userEvent, waitFor } from 'storybook/test';
import { DocsSearch } from './docs-search';
import { DocsService } from '../docs.service';
import { fakeDocsService } from '../docs-fixtures';
import { atPhoneWidth, inDarkTheme, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';

const withDocsService = applicationConfig({ providers: [provideRouter([{ path: '**', children: [] }]), provideLocationMocks(), { provide: DocsService, useValue: fakeDocsService() }] });

/** Types a query and waits for the results panel, which Gabarit floats over the page. */
async function searchFor(canvasElement: HTMLElement, query: string) {
  await userEvent.type(canvasElement.querySelector('input')!, query);
  const options = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll('[role="option"]'));
    if (found.length === 0) throw new Error('no results yet');
    return found;
  });
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
  return options;
}

const meta: Meta<DocsSearch> = {
  title: 'Docs/DocsSearch',
  component: DocsSearch,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  // Width of a docs nav column.
  decorators: [withFerrisgitIcons, withDocsService, inShellContentArea],
  render: () => ({ template: `<div style="max-width: 240px;"><fg-docs-search /></div>` }),
};

export default meta;
type Story = StoryObj<DocsSearch>;

export const Empty: Story = {};

export const Results: Story = {
  play: async ({ canvasElement }) => {
    const options = await searchFor(canvasElement, 'variables');
    await expect(options[0].querySelector('mark')?.textContent).toBe('variables');
  },
};

export const ResultsDark: Story = {
  decorators: [inDarkTheme],
  play: async ({ canvasElement }) => {
    await searchFor(canvasElement, 'variables');
  },
};

export const ResultsPhone: Story = {
  decorators: [atPhoneWidth],
  render: () => ({ template: `<fg-docs-search />` }),
  play: async ({ canvasElement }) => {
    await searchFor(canvasElement, 'docker');
  },
};

export const NoResult: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.type(canvasElement.querySelector('input')!, 'kubernetes');
    await waitFor(() => expect(canvasElement.textContent).toContain('Aucune page ne correspond.'));
  },
};
