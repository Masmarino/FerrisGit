import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { RunnersList } from './runners-list';
import { RunnerSummary, RunnersService } from '../runners.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, fakeToast } from '../../shared/layout/settings-story-helpers';

const secondsAgo = (seconds: number) => new Date(Date.now() - seconds * 1000).toISOString();

const RUNNERS: RunnerSummary[] = [
  { id: 'r1', name: 'vps-1', tags: ['docker', 'linux'], lastHeartbeatAt: secondsAgo(4), createdAt: daysAgo(40) },
  { id: 'r2', name: 'build-arm64-01', tags: ['docker', 'arm64', 'rust', 'cache'], lastHeartbeatAt: secondsAgo(40), createdAt: daysAgo(12) },
  { id: 'r3', name: 'gpu-box', tags: ['cuda'], lastHeartbeatAt: hoursAgo(5), createdAt: daysAgo(3) },
  { id: 'r4', name: 'runner-nouveau', tags: [], lastHeartbeatAt: null, createdAt: minutesAgo(20) },
  {
    id: 'r5',
    name: 'runner-de-la-salle-serveur-du-deuxième-étage-près-de-la-machine-à-café',
    tags: ['docker', 'linux', 'x86_64', 'large-disk', 'nightly'],
    lastHeartbeatAt: daysAgo(9),
    createdAt: daysAgo(90),
  },
];

function fakeRunnersService(list: RunnerSummary[], options: { refuse?: boolean } = {}): Pick<RunnersService, 'list' | 'register'> {
  let current = [...list];
  return {
    list: () => of(current),
    register: (name: string, tags: string[]): Observable<{ id: string; name: string; tags: string[]; token: string }> => {
      if (options.refuse) {
        return throwError(() => new Error('refused'));
      }
      current = [{ id: `new-${current.length}`, name, tags, lastHeartbeatAt: null, createdAt: new Date().toISOString() }, ...current];
      return of({ id: 'new-runner', name, tags, token: 'frg_rn_7f3c9a1e5b2d4c8f9a0b1c2d3e4f5a6b7c8d9e0f' });
    },
  };
}

const withRunners = (service: unknown) => moduleMetadata({ providers: [{ provide: RunnersService, useValue: service }] });

const rect = (el: Element) => el.getBoundingClientRect();

async function expectRunnersLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-list-row, gbt-empty-state')) throw new Error('runners not rendered yet');
  });
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);
  await expect(canvasElement.querySelectorAll('.gbt-button--primary'), 'one primary action').toHaveLength(1);
  await expectRows(canvasElement);
  for (const name of Array.from(canvasElement.querySelectorAll<HTMLElement>('.runners-list__name'))) {
    await expect(Math.round(rect(name).height), `"${name.textContent}" on one line`).toBeLessThanOrEqual(24);
  }
  for (const icon of Array.from(canvasElement.querySelectorAll('gbt-icon'))) {
    if (rect(icon).width > 0) {
      await expect(icon.querySelector('svg'), `icon in "${icon.parentElement?.textContent?.trim()}"`).not.toBeNull();
    }
  }
}

const meta: Meta<RunnersList> = {
  title: 'Runners/RunnersList',
  component: RunnersList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: RunnersService, useValue: fakeRunnersService(RUNNERS) },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
  play: ({ canvasElement }) => expectRunnersLayout(canvasElement),
};

export default meta;
type Story = StoryObj<RunnersList>;

export const Populated: Story = {};

export const NoRunners: Story = {
  decorators: [withRunners(fakeRunnersService([]))],
};

export const LoadFailed: Story = {
  decorators: [withRunners({ list: () => throwError(() => new Error('boom')), register: () => NEVER })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('.runners-list__failed')).not.toBeNull());
    await expect(canvasElement.querySelector('gbt-empty-state')).toBeNull();
  },
};

export const Loading: Story = {
  decorators: [withRunners({ list: () => NEVER, register: () => NEVER })],
  // Waits for the skeletons, not the page's layout guard (nothing is loaded yet).
  play: ({ canvasElement }) => waitFor(() => expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des runners…')),
};

export const TokenRevealed: Story = {
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(await page.findByRole('button', { name: 'Enregistrer un runner' }));
    await userEvent.type(await page.findByLabelText(/Nom du runner/), 'vps-3');
    await userEvent.click(page.getByRole('button', { name: 'Enregistrer' }));
    await waitFor(() => expect(canvasElement.querySelector('.runners-list__token')).not.toBeNull());
    await expect(canvasElement.querySelector('.runners-list__token gbt-secret-reveal code')?.textContent?.trim()).toMatch(/^frg_rn_/);
    await expectRunnersLayout(canvasElement);
  },
};

export const RegistrationRefused: Story = {
  decorators: [withRunners(fakeRunnersService(RUNNERS.slice(0, 2), { refuse: true }))],
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(await page.findByRole('button', { name: 'Enregistrer un runner' }));
    await userEvent.type(await page.findByLabelText(/Nom du runner/), 'vps-3');
    await userEvent.click(page.getByRole('button', { name: 'Enregistrer' }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('.runners-list__register gbt-alert')).not.toBeNull());
  },
};
