import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { AdminDashboard } from './admin-dashboard';
import { AdminMetricsService, AdminStats, MetricsSnapshot } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';
import { inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { fakeToast } from '../../shared/layout/settings-story-helpers';

const HOUR = 60 * 60 * 1000;

function history(days: number): MetricsSnapshot[] {
  const count = days * 4;
  return Array.from({ length: count }, (_, i) => {
    const progress = i / (count - 1);
    return {
      recordedAt: new Date(Date.now() - (count - 1 - i) * 6 * HOUR).toISOString(),
      totalUsers: 18 + Math.floor(progress * 24),
      totalRepositories: 41 + Math.floor(progress * 62 + Math.sin(i / 3) * 2),
      totalStorageBytes: Math.round((2.1 + progress * 3.4 + Math.sin(i / 5) * 0.08) * 1024 ** 3),
    };
  });
}

const STATS: AdminStats = { totalUsers: 42, totalRepositories: 103, pipelinesLast7Days: 1287 };

function metrics(overrides: Partial<Record<keyof AdminMetricsService, unknown>> = {}) {
  return {
    getStats: () => of(STATS),
    getHistory: (days: number) => of(history(days)),
    ...overrides,
  };
}

const withMetrics = (overrides: Partial<Record<keyof AdminMetricsService, unknown>> = {}) =>
  moduleMetadata({ providers: [{ provide: AdminMetricsService, useValue: metrics(overrides) }] });

const rect = (el: Element) => el.getBoundingClientRect();

async function expectDashboardLayout(canvasElement: HTMLElement) {
  const card = await waitFor(() => {
    const element = canvasElement.querySelector<HTMLElement>('.admin-dashboard__evolution');
    if (!element) throw new Error('dashboard not rendered yet');
    return element;
  });
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);

  const tiles = Array.from(canvasElement.querySelectorAll('gbt-stat-tile, .gbt-stat-grid__placeholder')).map(rect);
  await expect(new Set(tiles.map((t) => Math.round(t.height))).size, 'tiles of one height').toBe(1);
  const rows = new Set(tiles.map((t) => Math.round(t.top))).size;
  await expect(rows, 'rows of tiles').toBe(rect(canvasElement.querySelector('.gbt-stat-grid')!).width >= 600 ? 1 : 2);

  const header = rect(card.querySelector('.admin-dashboard__band')!);
  const range = rect(card.querySelector('.admin-dashboard__range')!);
  await expect(range.top, 'period control inside the header').toBeGreaterThanOrEqual(header.top);
  await expect(range.bottom, 'period control inside the header').toBeLessThanOrEqual(header.bottom);
  await expect(range.right, 'period control inside the header').toBeLessThanOrEqual(header.right);

  const panes = Array.from(card.querySelectorAll('.admin-dashboard__chart')).map(rect);
  if (panes.length === 2) {
    await expect(Math.round(panes[0].height), 'panes of one height').toBe(Math.round(panes[1].height));
    if (rect(card).width >= 760) {
      await expect(Math.round(panes[0].top), 'panes side by side').toBe(Math.round(panes[1].top));
    } else {
      await expect(panes[1].top, 'panes stacked').toBeGreaterThan(panes[0].bottom);
    }
  }
}

const meta: Meta<AdminDashboard> = {
  title: 'Admin/AdminDashboard',
  component: AdminDashboard,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: AdminMetricsService, useValue: metrics() },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
  play: ({ canvasElement }) => expectDashboardLayout(canvasElement),
};

export default meta;
type Story = StoryObj<AdminDashboard>;

export const Populated: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelectorAll('gbt-line-chart svg')).toHaveLength(2));
    await expectDashboardLayout(canvasElement);
  },
};

export const NoHistoryYet: Story = {
  decorators: [withMetrics({ getStats: () => of({ totalUsers: 1, totalRepositories: 0, pipelinesLast7Days: 0 }), getHistory: () => of(history(1).slice(-1)) })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelectorAll('.admin-dashboard__chart-empty')).toHaveLength(2));
    await expectDashboardLayout(canvasElement);
  },
};

export const Loading: Story = {
  decorators: [withMetrics({ getStats: () => NEVER, getHistory: () => NEVER })],
};

export const HistoryFailed: Story = {
  decorators: [withMetrics({ getHistory: () => throwError(() => new Error('boom')) })],
};
