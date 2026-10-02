import { Component, inject, provideAppInitializer } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { of } from 'rxjs';
import { RepositorySettings } from './repository-settings';
import { RepositoriesService } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';
import { CollaboratorSummary, RepositorySettingsService, WebhookSummary } from '../repository-settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { atPhoneWidth, daysAgo, hoursAgo, inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast } from '../../shared/layout/settings-story-helpers';

@Component({ template: '' })
class Blank {}

// In-memory navigation: the nav's links are relative to the page's route, so clicking a section moves to `/?section=<key>`.
const withRouter = applicationConfig({ providers: [provideRouter([{ path: '**', component: Blank }]), provideLocationMocks()] });
const startAt = (url: string) => applicationConfig({ providers: [provideAppInitializer(() => inject(Router).navigateByUrl(url))] });

const COLLABORATORS: CollaboratorSummary[] = [
  { userId: 'u1', username: 'camille.durand', role: 'maintainer', createdAt: daysAgo(120) },
  { userId: 'u2', username: 'julien.martin', role: 'contributor', createdAt: daysAgo(21) },
  { userId: 'u3', username: 'lucas.petit', role: 'reader', createdAt: hoursAgo(5) },
];

const WEBHOOKS: WebhookSummary[] = [
  { id: 'w1', url: 'https://ci.exemple.fr/hooks/ferrisgit', events: ['pipeline_failed', 'merge_request_merged'], active: true, createdAt: daysAgo(12) },
  {
    id: 'w2',
    url: 'https://chat.exemple.fr/api/webhooks/notifications',
    events: ['merge_request_approved', 'merge_request_commented', 'issue_assigned', 'issue_closed'],
    active: false,
    createdAt: daysAgo(45),
  },
];

const LABELS: Label[] = [
  { id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' },
  { id: 'l2', name: 'amélioration', color: '#2563eb', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-02T00:00:00Z' },
  { id: 'l3', name: 'documentation', color: '#16a34a', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-03T00:00:00Z' },
];

const MILESTONES: Milestone[] = [
  { id: 'm1', title: 'v1.0', description: '', dueDate: '2026-03-01T00:00:00Z', state: 'closed', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' },
  { id: 'm2', title: 'v1.1', description: '', dueDate: '2999-10-15T00:00:00Z', state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-02-01T00:00:00Z' },
];

const fakeRepositorySettingsService = {
  get: () => of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 1 }),
  update: () => of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 1 }),
  listCiVariables: () =>
    of([
      { id: 'v1', key: 'DATABASE_URL', masked: true },
      { id: 'v2', key: 'RUST_LOG', masked: false },
    ]),
  setCiVariable: () => of({ id: 'v3', key: 'NEW_VAR', masked: true }),
  deleteCiVariable: () => of(undefined),
  listCollaborators: () => of(COLLABORATORS),
  addCollaborator: () => of(undefined),
  setCollaboratorRole: () => of(undefined),
  removeCollaborator: () => of(undefined),
  listWebhooks: () => of(WEBHOOKS),
  createWebhook: () => of(WEBHOOKS[0]),
  deleteWebhook: () => of(undefined),
  listWebhookDeliveries: () => of([]),
};

const REPOSITORY = repositoryFixture({ description: 'Une plateforme Git auto-hébergée.', path: ['florian', 'ferrisgit'] });

const fakeRepositoriesService = { getById: () => of(REPOSITORY), update: () => of(REPOSITORY) };

const fakeLabelsService = { listForRepository: () => of(LABELS), create: () => of(LABELS[0]), delete: () => of(undefined) };
const fakeMilestonesService = { listForRepository: () => of(MILESTONES), create: () => of(MILESTONES[1]), delete: () => of(undefined) };

const SECTION_ELEMENTS: Record<string, string> = {
  Informations: 'fg-repository-general-settings',
  Pipeline: 'fg-repository-pipeline-settings',
  'Variables CI/CD': 'fg-repository-ci-variables',
  Webhooks: 'fg-repository-webhooks',
  Collaborateurs: 'fg-repository-collaborators-settings',
  Labels: 'fg-repository-labels-settings',
  Milestones: 'fg-repository-milestones-settings',
};

async function expectSettingsPage(canvasElement: HTMLElement, expected: string) {
  const active = await waitFor(() => {
    const link = canvasElement.querySelector<HTMLAnchorElement>('gbt-nav-tabs a[aria-current="page"]');
    if (!link || !canvasElement.querySelector('gbt-card')) {
      throw new Error('settings page not rendered yet');
    }
    return link;
  });
  await expect(canvasElement.querySelectorAll('nav'), 'navigation landmarks').toHaveLength(1);
  await expect(canvasElement.querySelector('nav')?.getAttribute('aria-label')).toBe('Réglages du dépôt');
  await expect(canvasElement.querySelectorAll('gbt-nav-tabs a[aria-current="page"]'), 'active links').toHaveLength(1);
  await expect(active.querySelector('.gbt-nav-tab__label')?.textContent, 'active section').toBe(expected);
  for (const [label, selector] of Object.entries(SECTION_ELEMENTS)) {
    await expect(canvasElement.querySelectorAll(selector), `${label} rendered`).toHaveLength(label === expected ? 1 : 0);
  }

  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return;
  }
  const layout = canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect();
  const nav = canvasElement.querySelector('.gbt-page-layout__nav')!.getBoundingClientRect();
  const main = canvasElement.querySelector('.gbt-page-layout__main')!.getBoundingClientRect();
  const h1 = canvasElement.querySelector('gbt-page-header h1')!.getBoundingClientRect();
  await expect(Math.round(h1.left), 'h1 on the nav column’s left edge').toBe(Math.round(nav.left));
  if (layout.width >= 769) {
    await expect(nav.right, 'nav column left of the section').toBeLessThanOrEqual(main.left);
    await expect(Math.round(nav.top), 'nav and section start together').toBe(Math.round(main.top));
    const links = Array.from(canvasElement.querySelectorAll<HTMLElement>('gbt-nav-tabs a'));
    await expect(new Set(links.map((a) => Math.round(a.getBoundingClientRect().top))).size, 'wide nav is a column').toBe(links.length);
    for (const link of links) {
      await expect(link.getBoundingClientRect().height, `height of "${link.textContent?.trim()}"`).toBeGreaterThanOrEqual(36);
    }
  } else {
    await expect(nav.bottom, 'nav stacked above the section').toBeLessThanOrEqual(main.top);
    const links = Array.from(canvasElement.querySelectorAll('gbt-nav-tabs a'));
    const tops = new Set(links.map((a) => Math.round(a.getBoundingClientRect().top)));
    await expect(tops.size, 'narrow nav is one row of tabs').toBe(1);
  }
  await expectSettingsLayout(canvasElement);
}

const meta: Meta<RepositorySettings> = {
  title: 'Repositories/Settings/RepositorySettings',
  component: RepositorySettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withRouter,
    moduleMetadata({
      providers: [
        { provide: RepositorySettingsService, useValue: fakeRepositorySettingsService },
        { provide: RepositoriesService, useValue: fakeRepositoriesService },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
        { provide: LabelsService, useValue: fakeLabelsService },
        { provide: MilestonesService, useValue: fakeMilestonesService },
      ],
    }),
    inShellContentArea,
  ],
  args: { repositoryId: 'repo-1' },
};

export default meta;
type Story = StoryObj<RepositorySettings>;

export const Informations: Story = {
  decorators: [startAt('/')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Informations'),
};

export const Pipeline: Story = {
  decorators: [startAt('/?section=pipeline')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Pipeline'),
};

export const WebhooksAtPhoneWidth: Story = {
  decorators: [startAt('/?section=webhooks'), atPhoneWidth],
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect().width).toBeLessThan(769);
    await expectSettingsPage(canvasElement, 'Webhooks');
  },
};

export const Variables: Story = {
  decorators: [startAt('/?section=variables')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Variables CI/CD'),
};

export const Webhooks: Story = {
  decorators: [startAt('/?section=webhooks')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Webhooks'),
};

export const Collaborators: Story = {
  decorators: [startAt('/?section=collaborators')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Collaborateurs'),
};

export const Labels: Story = {
  decorators: [startAt('/?section=labels')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Labels'),
};

export const Milestones: Story = {
  decorators: [startAt('/?section=milestones')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Milestones'),
};

export const UnknownSection: Story = {
  decorators: [startAt('/?section=avance')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Informations'),
};
