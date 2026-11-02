import { signal } from '@angular/core';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AccessTokensList } from './access-tokens-list';
import { AccessTokensService, AccessTokenSummary, NewAccessToken } from '../access-tokens.service';
import { MeService } from '../../shell/me.service';
import { daysAgo, hoursAgo, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const NEW_TOKEN = 'fga_5b0e1a7e5b2d4c86a0e1f7b3d92c5a48e6b1f0c27d34';

const TOKENS: AccessTokenSummary[] = [
  { id: 't1', name: 'fgit-portable', scopes: ['repositories:write', 'issues:write', 'merge-requests:write', 'pipelines:read', 'releases:read'], expiresAt: daysAgo(-60), expired: false, createdAt: daysAgo(30), lastUsedAt: minutesAgo(7) },
  { id: 't2', name: 'tableau-de-bord', scopes: ['pipelines:read'], expiresAt: daysAgo(-300), expired: false, createdAt: daysAgo(65), lastUsedAt: hoursAgo(20) },
  { id: 't3', name: 'script-de-sauvegarde', scopes: ['repositories:read'], expiresAt: daysAgo(-5), expired: false, createdAt: daysAgo(5), lastUsedAt: null },
  { id: 't4', name: 'ancien-agent-sur-le-serveur-de-build-de-la-salle-serveur-du-sous-sol', scopes: ['issues:read', 'merge-requests:read'], expiresAt: daysAgo(12), expired: true, createdAt: daysAgo(102), lastUsedAt: daysAgo(40) },
];

function fakeTokensService(overrides: Partial<Record<keyof AccessTokensService, unknown>> = {}) {
  return {
    list: () => of(TOKENS),
    create: (token: NewAccessToken) => of({ ...TOKENS[0], id: 't9', name: token.name, scopes: token.scopes, token: NEW_TOKEN }),
    revoke: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Partial<Record<keyof AccessTokensService, unknown>>) =>
  moduleMetadata({ providers: [{ provide: AccessTokensService, useValue: fakeTokensService(overrides) }] });

const meta: Meta<AccessTokensList> = {
  title: 'Account/AccessTokensList',
  component: AccessTokensList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: AccessTokensService, useValue: fakeTokensService() },
        { provide: MeService, useValue: { isAdmin: signal(false) } },
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
type Story = StoryObj<AccessTokensList>;

export const Populated: Story = {};

export const AsAdministrator: Story = {
  decorators: [moduleMetadata({ providers: [{ provide: MeService, useValue: { isAdmin: signal(true) } }] })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[data-area="admin"]');
    await expectSettingsLayout(canvasElement);
  },
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

export const CustomLifetime: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('radio', { name: 'Autre' }));
    await userEvent.type(await canvas.findByLabelText('Nombre de jours'), '400');
    await waitFor(() => expect(canvasElement.querySelector('.access-tokens-list__custom-lifetime')?.textContent).toContain('de 1 à 365'));
    await expectSettingsLayout(canvasElement);
  },
};

export const RevealedToken: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.type(await canvas.findByLabelText('Nom du jeton'), 'pipeline-de-release');
    await userEvent.click(within(await rendered(canvasElement, '[data-area="issues"]')).getByRole('radio', { name: 'Lecture' }));
    await userEvent.click(await canvas.findByRole('button', { name: 'Générer' }));
    const revealed = await rendered(canvasElement, '.access-tokens-list__revealed');
    await expect(revealed.querySelector('gbt-copy-field code')?.textContent).toBe(NEW_TOKEN);
    await waitFor(() => expect(canvasElement.ownerDocument.activeElement).toBe(revealed.querySelector('.gbt-card__title')));
    await expectSettingsLayout(canvasElement);
    await expectRows(canvasElement);
  },
};

export const ConfirmRevoke: Story = {
  play: async ({ canvasElement }) => {
    await userEvent.click(await rendered(canvasElement, '[aria-label="Révoquer le jeton fgit-portable"]'));
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
      if (!found) throw new Error('dialog not rendered yet');
      return found;
    });
    await expect(dialog.textContent).toContain('« fgit-portable »');
  },
};
