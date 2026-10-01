import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryWebhooks } from './repository-webhooks';
import { RepositorySettingsService, WebhookDelivery, WebhookSummary } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit';
import { daysAgo, hoursAgo, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const WEBHOOKS: WebhookSummary[] = [
  {
    id: 'w1',
    url: 'https://ci.exemple.fr/hooks/ferrisgit',
    events: ['pipeline_failed', 'merge_request_merged'],
    active: true,
    createdAt: daysAgo(12),
  },
  {
    id: 'w2',
    url: 'https://chat.exemple.fr/api/webhooks/notifications',
    events: ['merge_request_approved', 'merge_request_changes_requested', 'merge_request_commented', 'issue_assigned', 'issue_closed'],
    active: true,
    createdAt: daysAgo(45),
  },
  {
    id: 'w3',
    url: 'https://audit.exemple.fr/ferrisgit/collaborateurs/un-chemin-très-long-qui-ne-tient-pas-sur-une-ligne?token=abcdef0123456789',
    events: ['collaborator_added', 'collaborator_role_changed', 'collaborator_removed'],
    active: false,
    createdAt: daysAgo(90),
  },
];

const DELIVERIES: WebhookDelivery[] = [
  { id: 'd4', eventKind: 'merge_request_merged', httpStatus: null, success: false, errorMessage: 'Délai de connexion dépassé', createdAt: minutesAgo(12) },
  { id: 'd3', eventKind: 'pipeline_failed', httpStatus: 502, success: false, errorMessage: null, createdAt: hoursAgo(3) },
  { id: 'd2', eventKind: 'merge_request_merged', httpStatus: 200, success: true, errorMessage: null, createdAt: daysAgo(1) },
  { id: 'd1', eventKind: 'pipeline_failed', httpStatus: 200, success: true, errorMessage: null, createdAt: daysAgo(4) },
];

function fakeRepositorySettingsService(overrides: Record<string, unknown> = {}) {
  return {
    listWebhooks: () => of(WEBHOOKS),
    createWebhook: () => of(WEBHOOKS[0]),
    deleteWebhook: () => of(undefined),
    listWebhookDeliveries: () => of(DELIVERIES),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) =>
  moduleMetadata({ providers: [{ provide: RepositorySettingsService, useValue: fakeRepositorySettingsService(overrides) }] });

const meta: Meta<RepositoryWebhooks> = {
  title: 'Repositories/Settings/RepositoryWebhooks',
  component: RepositoryWebhooks,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: RepositorySettingsService, useValue: fakeRepositorySettingsService() },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inSettingsColumn,
  ],
  args: { repositoryId: 'repo-1' },
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSettingsLayout(canvasElement);
    await expectRows(canvasElement);
  },
};

export default meta;
type Story = StoryObj<RepositoryWebhooks>;

export const Populated: Story = {};

export const EventsSelected: Story = {
  play: async ({ canvasElement }) => {
    const boxes = await waitFor(() => {
      const found = canvasElement.querySelectorAll<HTMLInputElement>('.repository-webhooks__webhook-events input[type="checkbox"]');
      if (found.length === 0) {
        throw new Error('not rendered');
      }
      return Array.from(found);
    });
    await userEvent.click(boxes[3]);
    await userEvent.click(boxes[8]);
    await waitFor(() => expect(canvasElement.querySelector('.repository-webhooks__summary')?.textContent?.trim()).toBe('2 événements sélectionnés'));
    await expectSettingsLayout(canvasElement);
  },
};

export const DeliveryHistory: Story = {
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Historique des livraisons de https://ci.exemple.fr/hooks/ferrisgit"]');
    await userEvent.click(button);
    await waitFor(() => expect(canvasElement.ownerDocument.querySelectorAll('.repository-webhooks__delivery')).toHaveLength(4));
  },
};

export const EmptyDeliveryHistory: Story = {
  decorators: [withService({ listWebhookDeliveries: () => of([]) })],
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Historique des livraisons de https://ci.exemple.fr/hooks/ferrisgit"]');
    await userEvent.click(button);
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('.repository-webhooks__deliveries-empty')).not.toBeNull());
  },
};

export const DeliveryHistoryFailed: Story = {
  decorators: [withService({ listWebhookDeliveries: () => throwError(() => ({ status: 500 })) })],
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Historique des livraisons de https://ci.exemple.fr/hooks/ferrisgit"]');
    await userEvent.click(button);
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('gbt-drawer gbt-alert .gbt-alert[data-variant="error"]')).not.toBeNull());
  },
};

async function confirmDialog(canvasElement: HTMLElement): Promise<HTMLElement> {
  return waitFor(() => {
    const found = canvasElement.ownerDocument.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
    if (!found) throw new Error('dialog not rendered yet');
    return found;
  });
}

export const ConfirmDelete: Story = {
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, `[aria-label="Supprimer le webhook ${WEBHOOKS[2].url}"]`);
    await userEvent.click(button);
    const dialog = await confirmDialog(canvasElement);
    await expect(dialog.getAttribute('aria-label')).toBe('Supprimer le webhook');
    await expect(dialog.querySelector('input')).toBeNull();
    await expect(dialog.textContent).toContain(WEBHOOKS[2].url);

    const doc = canvasElement.ownerDocument.documentElement;
    const box = dialog.getBoundingClientRect();
    await expect(box.left, 'dialog left edge').toBeGreaterThanOrEqual(0);
    await expect(box.right, 'dialog right edge').toBeLessThanOrEqual(doc.clientWidth);
    await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);
    const message = dialog.querySelector<HTMLElement>('.gbt-confirm-danger-modal__message')!;
    await expect(message.scrollWidth, 'message wraps inside its box').toBeLessThanOrEqual(message.clientWidth + 1);
    const [cancel, confirm] = Array.from(dialog.querySelectorAll<HTMLElement>('.gbt-confirm-danger-modal__actions .gbt-button'));
    await expect(Math.round(cancel.getBoundingClientRect().top), 'buttons on one line').toBe(Math.round(confirm.getBoundingClientRect().top));
    await expect(confirm.getBoundingClientRect().left, 'cancel before confirm').toBeGreaterThan(cancel.getBoundingClientRect().right);
    await expect(confirm.getBoundingClientRect().right, 'confirm inside the dialog').toBeLessThanOrEqual(box.right);
    for (const icon of Array.from(dialog.querySelectorAll('gbt-icon'))) {
      await expect(icon.querySelector('svg'), 'registered icon').not.toBeNull();
    }
  },
};

export const ConfirmDeleteBusy: Story = {
  decorators: [withService({ deleteWebhook: () => NEVER })],
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, `[aria-label="Supprimer le webhook ${WEBHOOKS[2].url}"]`);
    await userEvent.click(button);
    const dialog = await confirmDialog(canvasElement);
    await userEvent.click(dialog.querySelector<HTMLElement>('.gbt-button--danger')!);
    await waitFor(() => expect(dialog.querySelector('.gbt-button--danger')?.getAttribute('aria-busy')).toBe('true'));
    await expect(dialog.querySelector<HTMLButtonElement>('.gbt-button--secondary')?.disabled).toBe(true);
    await expect(dialog.querySelector('[role="status"]')?.textContent?.trim()).toBe('Suppression en cours');
    await userEvent.keyboard('{Escape}');
    await expect(canvasElement.ownerDocument.querySelector('gbt-confirm-danger-modal [role="dialog"]')).not.toBeNull();
  },
};

export const Empty: Story = {
  decorators: [withService({ listWebhooks: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listWebhooks: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
    await expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des webhooks…');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listWebhooks: () => throwError(() => ({ status: 500 })) })],
};
