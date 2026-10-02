import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { expect } from 'storybook/test';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { PipelineSidebar } from './pipeline-sidebar';
import { NOW, failedGroups, runningGroups } from '../pipeline-story-fixtures';

const meta: Meta<PipelineSidebar> = {
  title: 'Pipelines/PipelineSidebar',
  component: PipelineSidebar,
  tags: ['autodocs'],
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] })],
  args: {
    path: ['acme', 'widget'],
    pipelineId: 'pipeline-1',
    groups: runningGroups,
    selectedJobId: null,
    now: NOW,
  },
};

export default meta;
type Story = StoryObj<PipelineSidebar>;

export const Running: Story = {
  play: async ({ canvasElement }) => {
    const glyphs = Array.from(canvasElement.querySelectorAll('.pipeline-sidebar__link gbt-job-status'));
    await expect(glyphs.map((glyph) => glyph.getAttribute('data-status'))).toEqual(['success', 'success', 'success', 'running', 'pending']);
    await expect(glyphs.map((glyph) => glyph.querySelector('.sr-only')?.textContent?.trim())).toEqual(['Réussi', 'Réussi', 'Réussi', 'En cours', 'En attente']);
  },
};

export const Failed: Story = {
  args: { groups: failedGroups },
  play: async ({ canvasElement }) => {
    const glyphs = Array.from(canvasElement.querySelectorAll('.pipeline-sidebar__link gbt-job-status'));
    await expect(glyphs.map((glyph) => glyph.getAttribute('data-status'))).toEqual(['success', 'success', 'success', 'failed', 'canceled']);
    await expect(glyphs.map((glyph) => glyph.querySelector('.sr-only')?.textContent?.trim())).toEqual(['Réussi', 'Réussi', 'Réussi', 'Échoué', 'Ignoré']);
  },
};

export const JobSelected: Story = {
  args: { groups: failedGroups, selectedJobId: 'job-4' },
};
