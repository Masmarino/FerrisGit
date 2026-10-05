import { Component, inject, provideAppInitializer, signal } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { Observable, of, throwError, timer } from 'rxjs';
import { switchMap } from 'rxjs/operators';
import { AccountPage } from './account-page';
import { AuthService } from '../../auth/auth.service';
import { Me, MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { type Passkey } from '@masmarino/gabarit/auth';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { MfaService } from '../../auth/mfa.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';
import { fakeMfaService, withMfa } from '../mfa-story-helpers';
import { ApiTokenSummary, TokensService } from '../../api-tokens/api-tokens.service';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast } from '../../shared/layout/settings-story-helpers';

@Component({ template: '' })
class Blank {}

// In-memory navigation: the nav links are relative to the route, so a click goes to `/?section=<key>`.
const withRouter = applicationConfig({ providers: [provideRouter([{ path: '**', component: Blank }]), provideLocationMocks()] });
const startAt = (url: string) => applicationConfig({ providers: [provideAppInitializer(() => inject(Router).navigateByUrl(url))] });

const ME: Me = { id: 'u1', username: 'florian.simon', email: 'florian@exemple.fr', isAdmin: false, createdAt: '2025-03-12T09:00:00Z' };

// AuthService needs HttpClient and is created eagerly with the page, so use a no-op fake.
const fakeAuthService: Pick<AuthService, 'logout' | 'setToken' | 'authConfig'> = {
  logout: () => {},
  setToken: () => {},
  authConfig: () => of({ registrationEnabled: false, passkeysAvailable: true }),
};

/** Arrives later, like a real HTTP error. */
const failLater = <T>(status: number): Observable<T> => timer(400).pipe(switchMap(() => throwError(() => ({ status }))));

function fakeMeService(me: Me, overrides: Partial<Pick<MeService, 'updateEmail' | 'changePassword'>> = {}) {
  const email = signal(me.email);
  return {
    id: signal(me.id),
    username: signal(me.username),
    email,
    isAdmin: signal(me.isAdmin),
    createdAt: signal(me.createdAt),
    load: () => {},
    updateEmail: (value: string) => {
      email.set(value);
      return of({ ...me, email: value });
    },
    changePassword: () => of(undefined),
    ...overrides,
  };
}

const TOKENS: ApiTokenSummary[] = [
  { id: 't1', name: 'ci-github-actions', createdAt: daysAgo(44), lastUsedAt: minutesAgo(7) },
  { id: 't2', name: 'poste-de-travail-florian', createdAt: daysAgo(114), lastUsedAt: hoursAgo(20) },
  { id: 't3', name: 'script-de-sauvegarde', createdAt: daysAgo(5), lastUsedAt: null },
];

function fakeTokensService(list: ApiTokenSummary[]): Pick<TokensService, 'list' | 'create' | 'revoke'> {
  return {
    list: () => of(list),
    create: (name: string) => of({ id: 'new-token', name, token: 'fgt_4b7e0c9a1d2f38e6a5c1b09d7f3e2a64c8d15b90' }),
    revoke: () => of(undefined),
  };
}

const PASSKEY: Passkey = { id: 'p1', name: 'MacBook Touch ID', createdAt: '2025-03-12T09:30:00Z', lastUsedAt: hoursAgo(3) };

const withMe = (me: Me, overrides: Partial<Pick<MeService, 'updateEmail' | 'changePassword'>> = {}) =>
  moduleMetadata({ providers: [{ provide: MeService, useFactory: () => fakeMeService(me, overrides) }] });
const withTokens = (list: ApiTokenSummary[]) => moduleMetadata({ providers: [{ provide: TokensService, useValue: fakeTokensService(list) }] });

async function expectAccountPage(canvasElement: HTMLElement, expected: string) {
  const active = await waitFor(() => {
    const link = canvasElement.querySelector<HTMLAnchorElement>('gbt-nav-tabs a[aria-current="page"]');
    if (!link || !canvasElement.querySelector('gbt-card')) {
      throw new Error('account page not rendered yet');
    }
    return link;
  });
  await expect(canvasElement.querySelectorAll('nav'), 'navigation landmarks').toHaveLength(1);
  await expect(canvasElement.querySelector('nav')?.getAttribute('aria-label')).toBe('Réglages du compte');
  await expect(canvasElement.querySelectorAll('gbt-nav-tabs a[aria-current="page"]'), 'active links').toHaveLength(1);
  await expect(active.querySelector('.gbt-nav-tab__label')?.textContent, 'active section').toBe(expected);
  await expect(canvasElement.querySelectorAll('.gbt-button--primary').length, 'primary buttons').toBeLessThanOrEqual(1);

  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  const layout = canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect();
  const navBox = canvasElement.querySelector('.gbt-page-layout__nav')!;
  const nav = navBox.getBoundingClientRect();
  const main = canvasElement.querySelector('.gbt-page-layout__main')!.getBoundingClientRect();
  const h1 = canvasElement.querySelector('gbt-page-header h1')!.getBoundingClientRect();
  await expect(Math.round(h1.left), 'h1 on the nav column’s left edge').toBe(Math.round(nav.left));
  if (layout.width >= 769) {
    await expect(nav.right, 'nav column left of the section').toBeLessThanOrEqual(main.left);
    await expect(Math.round(nav.top), 'nav and section start together').toBe(Math.round(main.top));
    await expect(getComputedStyle(navBox).position, 'sticky nav').toBe('sticky');
  } else {
    await expect(nav.bottom, 'nav stacked above the section').toBeLessThanOrEqual(main.top);
    const links = Array.from(canvasElement.querySelectorAll('gbt-nav-tabs a'));
    const tops = new Set(links.map((a) => Math.round(a.getBoundingClientRect().top)));
    await expect(tops.size, 'narrow nav is one row of tabs').toBe(1);
  }
  await expectSettingsLayout(canvasElement);
}

async function expectPasswordForm(canvasElement: HTMLElement) {
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return;
  }
  const inputs = Array.from(canvasElement.querySelectorAll<HTMLElement>('.account-page__password-form .gbt-input__wrapper input'));
  await expect(inputs).toHaveLength(3);
  const heights = new Set(inputs.map((input) => Math.round(input.getBoundingClientRect().height)));
  await expect(heights.size, 'one field height').toBe(1);
  const [, next, confirm] = inputs.map((input) => input.getBoundingClientRect());
  const card = canvasElement.querySelector('gbt-card')!.getBoundingClientRect();
  if (card.width >= 520) {
    await expect(Math.round(next.top), 'new and confirmation side by side').toBe(Math.round(confirm.top));
  } else {
    await expect(confirm.top, 'stacked in a narrow card').toBeGreaterThan(next.bottom);
  }
}

const meta: Meta<AccountPage> = {
  title: 'Account/AccountPage',
  component: AccountPage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withRouter,
    moduleMetadata({
      providers: [
        { provide: AuthService, useValue: fakeAuthService },
        // A factory, so every story starts from the same profile whatever a previous one saved.
        { provide: MeService, useFactory: () => fakeMeService(ME) },
        { provide: TokensService, useValue: fakeTokensService(TOKENS) },
        { provide: MfaService, useFactory: () => fakeMfaService({ totpEnabled: true, backupCodesRemaining: 8, passkeys: [PASSKEY] }) },
        provideFerrisgitAuth(),
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
};

export default meta;
type Story = StoryObj<AccountPage>;

export const Profile: Story = {
  decorators: [startAt('/')],
  play: ({ canvasElement }) => expectAccountPage(canvasElement, 'Profil'),
};

export const ProfileOfAnAdministrator: Story = {
  decorators: [startAt('/'), withMe({ ...ME, isAdmin: true })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Profil');
    await expect(canvasElement.querySelector('.account-page__role')?.textContent?.trim()).toBe('Super-administrateur');
  },
};

export const EmailSaved: Story = {
  decorators: [startAt('/')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Profil');
    const email = within(canvasElement).getByLabelText('Email');
    await userEvent.clear(email);
    await userEvent.type(email, 'florian.simon@exemple.fr');
    await userEvent.tab();
    await waitFor(() => expect(canvasElement.querySelector('.account-page__save-state [role="status"]')?.getAttribute('data-state')).toBe('saved'));
  },
};

export const EmailSaveFailed: Story = {
  decorators: [startAt('/'), withMe(ME, { updateEmail: () => failLater(500) })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Profil');
    const email = within(canvasElement).getByLabelText('Email') as HTMLInputElement;
    await userEvent.clear(email);
    await userEvent.type(email, 'florian.simon@exemple.fr');
    await userEvent.tab();
    await waitFor(() => expect(canvasElement.querySelector('.account-page__save-state [role="status"]')?.getAttribute('data-state')).toBe('error'));
    await waitFor(() => expect(email.value).toBe(ME.email));
  },
};

export const Password: Story = {
  decorators: [startAt('/?section=password')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Mot de passe');
    await expectPasswordForm(canvasElement);
  },
};

export const PasswordMismatch: Story = {
  decorators: [startAt('/?section=password')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Mot de passe');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Mot de passe actuel'), 'ancien-mot-de-passe');
    await userEvent.type(canvas.getByLabelText('Nouveau mot de passe'), 'nouveau-mot-de-passe');
    await userEvent.type(canvas.getByLabelText('Confirmer le nouveau mot de passe'), 'nouveau-mot-de-pase');
    await userEvent.click(canvas.getByRole('button', { name: 'Changer le mot de passe' }));
    await waitFor(() => expect(canvasElement.querySelector('.account-page__confirm .gbt-input__error')).not.toBeNull());
    await expect(canvasElement.querySelectorAll('.gbt-button--primary')).toHaveLength(1);
    await expectPasswordForm(canvasElement);
  },
};

export const PasswordChanged: Story = {
  decorators: [startAt('/?section=password')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Mot de passe');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Mot de passe actuel'), 'ancien-mot-de-passe');
    await userEvent.type(canvas.getByLabelText('Nouveau mot de passe'), 'nouveau-mot-de-passe');
    await userEvent.type(canvas.getByLabelText('Confirmer le nouveau mot de passe'), 'nouveau-mot-de-passe');
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(canvasElement.querySelector('.account-page__password-status [role="status"]')?.getAttribute('data-state')).toBe('saved'));
  },
};

export const PasswordRefused: Story = {
  decorators: [startAt('/?section=password'), withMe(ME, { changePassword: () => failLater(400) })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Mot de passe');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Mot de passe actuel'), 'pas-le-bon');
    await userEvent.type(canvas.getByLabelText('Nouveau mot de passe'), 'nouveau-mot-de-passe');
    await userEvent.type(canvas.getByLabelText('Confirmer le nouveau mot de passe'), 'nouveau-mot-de-passe');
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(canvasElement.querySelector('.account-page__password-status [role="status"]')?.getAttribute('data-state')).toBe('error'));
  },
};

export const Security: Story = {
  decorators: [startAt('/?section=security')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Sécurité');
    await waitFor(() => expect(canvasElement.querySelector('fg-mfa-settings')?.textContent).toContain('Activée'));
    await waitFor(() => expect(canvasElement.querySelector('fg-passkey-settings')?.textContent).toContain('MacBook Touch ID'));
    await expectSettingsLayout(canvasElement);
  },
};

export const SecurityPasskeyOnly: Story = {
  decorators: [startAt('/?section=security'), withMfa({ totpEnabled: false, backupCodesRemaining: 10, passkeys: [PASSKEY] })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Sécurité');
    await waitFor(() => expect(canvasElement.querySelector('fg-mfa-settings')?.textContent).toContain('Aucune application configurée'));
    await waitFor(() => expect(canvasElement.querySelector('fg-passkey-settings')?.textContent).toContain('MacBook Touch ID'));
    await expectSettingsLayout(canvasElement);
  },
};

export const SecurityAppOnly: Story = {
  decorators: [startAt('/?section=security'), withMfa({ totpEnabled: true, backupCodesRemaining: 8, passkeys: [] })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Sécurité');
    await waitFor(() => expect(canvasElement.querySelector('fg-passkey-settings')?.textContent).toContain("Aucune clé d'accès"));
    await expectSettingsLayout(canvasElement);
  },
};

export const SecurityWithoutFactor: Story = {
  decorators: [startAt('/?section=security'), withMfa({ totpEnabled: false, backupCodesRemaining: 0, passkeys: [] })],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Sécurité');
    await waitFor(() => expect(canvasElement.querySelector('fg-mfa-settings')?.textContent).toContain('La double authentification est obligatoire'));
  },
};

export const Tokens: Story = {
  decorators: [startAt('/?section=tokens')],
  play: ({ canvasElement }) => expectAccountPage(canvasElement, 'Jetons Git'),
};

export const NoTokens: Story = {
  decorators: [startAt('/?section=tokens'), withTokens([])],
  play: ({ canvasElement }) => expectAccountPage(canvasElement, 'Jetons Git'),
};

export const TokenRevealed: Story = {
  decorators: [startAt('/?section=tokens')],
  play: async ({ canvasElement }) => {
    await expectAccountPage(canvasElement, 'Jetons Git');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Nom du jeton'), 'pipeline-de-release');
    await userEvent.click(canvas.getByRole('button', { name: 'Générer' }));
    await waitFor(() => expect(canvasElement.querySelector('.api-tokens-list__revealed')).not.toBeNull());
    await expectAccountPage(canvasElement, 'Jetons Git');
  },
};

export const UnknownSection: Story = {
  decorators: [startAt('/?section=avance')],
  play: ({ canvasElement }) => expectAccountPage(canvasElement, 'Profil'),
};
