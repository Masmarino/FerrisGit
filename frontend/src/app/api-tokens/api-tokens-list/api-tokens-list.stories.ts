import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ApiTokensList } from './api-tokens-list';
import { ApiTokenSummary, TokensService } from '../api-tokens.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { daysAgo, hoursAgo, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const NEW_TOKEN = 'fgt_9f3c1a7e5b2d4c86a0e1f7b3d92c5a48e6b1f0c27d34';

const TOKENS: ApiTokenSummary[] = [
  { id: 't1', name: 'ci-github-actions', createdAt: daysAgo(44), lastUsedAt: minutesAgo(7) },
  { id: 't2', name: 'poste-de-travail-florian', createdAt: daysAgo(114), lastUsedAt: hoursAgo(20) },
  { id: 't3', name: 'script-de-sauvegarde', createdAt: daysAgo(5), lastUsedAt: null },
  { id: 't4', name: 'ancien-runner-jenkins-sur-le-serveur-de-build-de-la-salle-serveur-du-sous-sol', createdAt: daysAgo(690), lastUsedAt: daysAgo(620) },
];

const MANY_TOKENS: ApiTokenSummary[] = Array.from({ length: 12 }, (_, i) => ({
  id: `m${i + 1}`,
  name: `deploiement-env-${String(i + 1).padStart(2, '0')}`,
  createdAt: daysAgo(30 + i * 9),
  lastUsedAt: i % 3 === 0 ? null : hoursAgo(i * 5 + 1),
}));

function fakeTokensService(overrides: Partial<Record<keyof TokensService, unknown>> = {}) {
  return {
    list: () => of(TOKENS),
    create: (name: string) => of({ id: 't9', name, token: NEW_TOKEN }),
    revoke: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Partial<Record<keyof TokensService, unknown>>) =>
  moduleMetadata({ providers: [{ provide: TokensService, useValue: fakeTokensService(overrides) }] });

const meta: Meta<ApiTokensList> = {
  title: 'Account/ApiTokensList',
  component: ApiTokensList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: TokensService, useValue: fakeTokensService() },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inSettingsColumn,
  ],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSettingsLayout(canvasElement);
    await expectRows(canvasElement);
  },
};

export default meta;
type Story = StoryObj<ApiTokensList>;

export const Populated: Story = {};

export const ManyTokens: Story = {
  decorators: [withService({ list: () => of(MANY_TOKENS) })],
};

export const Empty: Story = {
  decorators: [withService({ list: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ list: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
    await expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des jetons…');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ list: () => throwError(() => ({ status: 500 })) })],
};

export const RevealedToken: Story = {
  decorators: [withService({ list: () => of([...TOKENS, { id: 't9', name: 'pipeline-de-release', createdAt: minutesAgo(0), lastUsedAt: null }]) })],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(await canvas.findByLabelText('Nom du jeton'), 'pipeline-de-release');
    await userEvent.click(await canvas.findByRole('button', { name: 'Générer' }));
    const revealed = await rendered(canvasElement, '.api-tokens-list__revealed');
    await expect(revealed.querySelector('gbt-copy-field code')?.textContent).toBe(NEW_TOKEN);
    await waitFor(() => expect(canvasElement.ownerDocument.activeElement).toBe(revealed.querySelector('.gbt-card__title')));
    await expect(canvas.getByRole('button', { name: 'Copier le jeton' })).toBeTruthy();
    await expect((await canvas.findByLabelText('Nom du jeton')) as HTMLInputElement).toHaveValue('');
    await expectSettingsLayout(canvasElement);
    await expectRows(canvasElement);

    const doc = canvasElement.ownerDocument.documentElement;
    if (doc.clientWidth > 0) {
      const cards = Array.from(canvasElement.querySelectorAll('gbt-card'));
      await expect(cards.indexOf(revealed.closest('gbt-card')!), 'reveal card position').toBe(1);
      const title = revealed.querySelector<HTMLElement>(':scope > .gbt-card__header .gbt-card__title')!;
      const listTitle = cards[2].querySelector<HTMLElement>(':scope > .gbt-card__header .gbt-card__title')!;
      await expect(getComputedStyle(title).color, 'success-toned heading').not.toBe(getComputedStyle(listTitle).color);
    }
  },
};

export const ConfirmRevoke: Story = {
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Révoquer le jeton ci-github-actions"]');
    await userEvent.click(button);
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
      if (!found) throw new Error('dialog not rendered yet');
      return found;
    });
    await expect(dialog.getAttribute('aria-label')).toBe('Révoquer le jeton');
    await expect(dialog.textContent).toContain('« ci-github-actions »');
    await expect(dialog.querySelector('input')).toBeNull();
  },
};
