import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { AdminHealth } from './admin-health';
import { AdminMetricsService, HealthStatus } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { fakeToast } from '../../shared/layout/settings-story-helpers';

const ALL_UP: HealthStatus = {
  database: { status: 'up', detail: null, responseTimeMs: 3, activeConnections: 7, maxConnections: 100, serverVersion: '18.0' },
  storage: { status: 'up', detail: null, usedBytes: 41_000_000_000, freeBytes: 209_000_000_000, totalBytes: 250_000_000_000 },
  uptimeSeconds: 2 * 86_400 + 3 * 3600 + 17 * 60,
};

const withHealth = (getHealth: () => unknown) => moduleMetadata({ providers: [{ provide: AdminMetricsService, useValue: { getHealth } }] });

const rect = (el: Element) => el.getBoundingClientRect();

async function expectHealthLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('.admin-health__card, gbt-alert')) throw new Error('health not rendered yet');
  });
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);

  const cards = Array.from(canvasElement.querySelectorAll('.admin-health__card')).map(rect);
  if (cards.length === 2) {
    if (rect(canvasElement.querySelector('.admin-health__cards')!).width >= 640) {
      await expect(Math.round(cards[0].top), 'cards side by side').toBe(Math.round(cards[1].top));
      await expect(Math.round(cards[0].height), 'cards of one height').toBe(Math.round(cards[1].height));
    } else {
      await expect(cards[1].top, 'cards stacked').toBeGreaterThan(cards[0].bottom);
    }
  }
  for (const card of Array.from(canvasElement.querySelectorAll('.admin-health__card'))) {
    const header = rect(card.querySelector('.gbt-card__header')!);
    const badge = rect(card.querySelector('.admin-health__card-status')!);
    await expect(badge.bottom, 'status badge in the header').toBeLessThanOrEqual(header.bottom);
    await expect(badge.right, 'status badge in the header').toBeLessThanOrEqual(header.right);
  }
  for (const icon of Array.from(canvasElement.querySelectorAll('gbt-icon'))) {
    if (rect(icon).width > 0) {
      await expect(icon.querySelector('svg'), `icon in "${icon.parentElement?.textContent?.trim()}"`).not.toBeNull();
    }
  }
}

const meta: Meta<AdminHealth> = {
  title: 'Admin/AdminHealth',
  component: AdminHealth,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: AdminMetricsService, useValue: { getHealth: () => of(ALL_UP) } },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
  play: ({ canvasElement }) => expectHealthLayout(canvasElement),
};

export default meta;
type Story = StoryObj<AdminHealth>;

export const AllUp: Story = {};

export const DatabaseDown: Story = {
  decorators: [
    withHealth(() =>
      of({
        ...ALL_UP,
        database: { status: 'down', detail: 'connection refused (os error 111)', responseTimeMs: 0, activeConnections: 0, maxConnections: 100, serverVersion: null },
        uptimeSeconds: 120,
      }),
    ),
  ],
};

export const StorageAlmostFull: Story = {
  decorators: [
    withHealth(() =>
      of({
        ...ALL_UP,
        database: { ...ALL_UP.database, activeConnections: 88 },
        storage: { status: 'up', detail: null, usedBytes: 242_000_000_000, freeBytes: 8_000_000_000, totalBytes: 250_000_000_000 },
      }),
    ),
  ],
};

export const Unreachable: Story = {
  decorators: [withHealth(() => throwError(() => new Error('boom')))],
};

export const Loading: Story = {
  decorators: [withHealth(() => NEVER)],
  // Waits for the skeletons, not the page's layout guard (nothing is loaded yet).
  play: ({ canvasElement }) => waitFor(() => expect(canvasElement.querySelector('.admin-health__cards-frame[aria-busy="true"]')).not.toBeNull()),
};
