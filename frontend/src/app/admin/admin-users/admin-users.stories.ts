import { HttpErrorResponse } from '@angular/common/http';
import { signal } from '@angular/core';
import { provideRouter } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { delay, NEVER, Observable, of, throwError } from 'rxjs';
import { AdminUsers } from './admin-users';
import { AdminUser, AdminUsersService, InviteResult, PasswordResetResult } from '../admin-users.service';
import { AuthService } from '../../auth/auth.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { fakeToast } from '../../shared/layout/settings-story-helpers';

const inHours = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();

const user = (overrides: Partial<AdminUser> & Pick<AdminUser, 'id' | 'username'>): AdminUser => ({
  email: `${overrides.username}@ferrisgit.dev`,
  isAdmin: false,
  createdAt: daysAgo(10),
  state: 'active',
  invitationExpiresAt: null,
  mfaEnabled: true,
  ...overrides,
});

const USERS: AdminUser[] = [
  user({ id: 'u1', username: 'alice', isAdmin: true, createdAt: daysAgo(120) }),
  user({ id: 'u2', username: 'bob', createdAt: daysAgo(45) }),
  user({ id: 'u3', username: 'carol', email: 'carol.dupont@example.org', mfaEnabled: false, createdAt: daysAgo(6) }),
  user({ id: 'u4', username: 'dave', state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(19), createdAt: hoursAgo(5) }),
  user({ id: 'u5', username: 'erin', state: 'invited', mfaEnabled: false, invitationExpiresAt: daysAgo(2), createdAt: daysAgo(4) }),
  user({ id: 'u6', username: 'frank-admin', isAdmin: true, state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(2), createdAt: minutesAgo(90) }),
  user({
    id: 'u7',
    username: 'a-really-long-username-32-chars-',
    email: 'a-really-long-username-32-chars-@a-quite-long-company-domain.example.com',
    createdAt: daysAgo(60),
  }),
];

const ACTIVATION_URL = 'https://ferrisgit.example.com/activate#token=3f9c1a7be25d4c8e9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5';
const RESET_URL = 'https://ferrisgit.example.com/reset-password#token=9b8a7c6d5e4f30211203f4e5d6c7b8a99a8b7c6d5e4f3021a1b2c3d4e5f60718';

interface FakeOptions {
  mailFails?: boolean;
  refuseReset?: boolean;
  lastAdmin?: boolean;
}

function fakeUsersService(list: AdminUser[], options: FakeOptions = {}): Pick<AdminUsersService, 'list' | 'invite' | 'resend' | 'resetMfa' | 'resetPassword' | 'setAdmin'> {
  let current = [...list];
  const renewed = (id: string, emailSent: boolean): Observable<InviteResult> => {
    const found = current.find((item) => item.id === id)!;
    const updated: AdminUser = { ...found, invitationExpiresAt: inHours(24) };
    current = current.map((item) => (item.id === id ? updated : item));
    const result: InviteResult = emailSent
      ? { user: updated, emailSent: true }
      : { user: updated, emailSent: false, emailError: 'connection refused (smtp.mailgun.org:587)', activationUrl: ACTIVATION_URL };
    return of(result).pipe(delay(800));
  };
  return {
    list: () => of(current),
    invite: (username, email, isAdmin) => of({ user: user({ id: 'new', username, email, isAdmin, state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(24) }), emailSent: true }),
    resend: (id) => renewed(id, !options.mailFails),
    resetMfa: (id) => {
      if (options.refuseReset) {
        return throwError(() => new Error('refused'));
      }
      current = current.map((item) => (item.id === id ? { ...item, mfaEnabled: false } : item));
      return of(undefined).pipe(delay(400));
    },
    resetPassword: () => {
      const result: PasswordResetResult = options.mailFails
        ? { emailSent: false, emailError: 'connection refused (smtp.mailgun.org:587)', resetUrl: RESET_URL }
        : { emailSent: true };
      return of(result).pipe(delay(600));
    },
    setAdmin: (id, isAdmin) => {
      if (!isAdmin && options.lastAdmin) {
        return throwError(() => new HttpErrorResponse({ status: 409, error: { error: 'cannot remove the last administrator' } }));
      }
      current = current.map((item) => (item.id === id ? { ...item, isAdmin } : item));
      return of(undefined).pipe(delay(400));
    },
  };
}

const withUsers = (service: unknown) => moduleMetadata({ providers: [{ provide: AdminUsersService, useValue: service }] });
const withRouterStub = applicationConfig({ providers: [provideRouter([])] });

const rect = (el: Element) => el.getBoundingClientRect();

async function expectUsersLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-list-row, gbt-empty-state')) throw new Error('users not rendered yet');
  });
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);
  await expect(canvasElement.querySelectorAll('.gbt-button--primary'), 'one primary action').toHaveLength(1);

  const card = canvasElement.querySelector('gbt-list-card');
  if (card) {
    const cardBox = rect(card);
    const narrow = cardBox.width < 560;
    const rights = new Set<number>();
    for (const row of Array.from(canvasElement.querySelectorAll<HTMLElement>('gbt-list-row'))) {
      const trailing = row.querySelector<HTMLElement>('.admin-users__trailing')!;
      rights.add(Math.round(rect(trailing).right));
      for (const child of Array.from(row.querySelectorAll<HTMLElement>('*'))) {
        const box = rect(child);
        if (box.width === 0 || getComputedStyle(child).position === 'fixed' || child.closest('.sr-only, gbt-menu .gbt-menu__list')) {
          continue;
        }
        await expect(box.right, `${child.tagName.toLowerCase()}.${child.className} inside its row`).toBeLessThanOrEqual(cardBox.right + 0.5);
      }
      const trigger = row.querySelector<HTMLElement>('.gbt-menu__trigger');
      if (trigger) {
        const box = rect(trigger);
        await expect(box.right, 'kebab inside the card').toBeLessThanOrEqual(cardBox.right);
        await expect(box.left, 'kebab inside the card').toBeGreaterThanOrEqual(cardBox.left);
        if (narrow) {
          await expect(Math.round(box.width), 'kebab width on a phone').toBeGreaterThanOrEqual(44);
          await expect(Math.round(box.height), 'kebab height on a phone').toBeGreaterThanOrEqual(44);
        }
      }
      const title = row.querySelector<HTMLElement>('.gbt-user-chip');
      if (title) {
        await expect(Math.round(rect(title).height), 'username on one line').toBeLessThanOrEqual(24);
      }
    }
    await expect(rights.size, 'trailing columns on one right edge').toBeLessThanOrEqual(1);
  }
  for (const icon of Array.from(canvasElement.querySelectorAll('gbt-icon'))) {
    if (rect(icon).width > 0) {
      await expect(icon.querySelector('svg'), `icon in "${icon.parentElement?.textContent?.trim()}"`).not.toBeNull();
    }
  }
}

async function pickAction(canvasElement: HTMLElement, username: string, name: string) {
  const page = within(canvasElement);
  await userEvent.click(await page.findByRole('button', { name: `Actions pour ${username}` }));
  await userEvent.click(await page.findByRole('menuitem', { name }));
}

const meta: Meta<AdminUsers> = {
  title: 'Admin/AdminUsers',
  component: AdminUsers,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withRouterStub,
    moduleMetadata({
      providers: [
        { provide: AdminUsersService, useValue: fakeUsersService(USERS) },
        { provide: MeService, useValue: { id: signal('u1'), load: () => {} } },
        { provide: AuthService, useValue: { logout: () => {} } },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
  play: ({ canvasElement }) => expectUsersLayout(canvasElement),
};

export default meta;
type Story = StoryObj<AdminUsers>;

export const Default: Story = {};

export const Loading: Story = {
  decorators: [withUsers({ list: () => NEVER })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-card [aria-busy="true"]')).not.toBeNull());
    await expect(canvasElement.querySelector('gbt-list-toolbar')?.hasAttribute('inert')).toBe(true);
  },
};

export const Empty: Story = {
  decorators: [withUsers(fakeUsersService([]))],
};

export const LoadFailed: Story = {
  decorators: [withUsers({ list: () => throwError(() => new Error('boom')) })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-card [role="alert"]')).not.toBeNull());
    await expect(canvasElement.querySelectorAll('[role="alert"]').length, 'one alert').toBe(1);
    await expect(canvasElement.querySelector('gbt-list-card .gbt-empty-state__illustration')).toBeNull();
  },
};

export const ResendResultMailFailed: Story = {
  decorators: [withUsers(fakeUsersService(USERS, { mailFails: true }))],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'dave', "Renvoyer l'invitation");
    await waitFor(() => expect(canvasElement.querySelector('.admin-users__mail-failed')).not.toBeNull(), { timeout: 3000 });
    await expectUsersLayout(canvasElement);
  },
};

export const ResetConfirm: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'bob', 'Réinitialiser la double authentification');
    await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
  },
};

export const SelfReset: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'alice', 'Réinitialiser la double authentification');
    await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
    await expect(canvasElement.querySelector('.gbt-confirm-danger-modal__message')?.textContent).toContain('votre propre compte');
  },
};

export const Filtered: Story = {
  play: async ({ canvasElement }) => {
    const page = within(canvasElement);
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await userEvent.click(await page.findByRole('radio', { name: /Invitations en attente/ }));
    await waitFor(() => expect(canvasElement.querySelectorAll('gbt-list-row')).toHaveLength(3));
    await userEvent.type(page.getByRole('textbox', { name: 'Rechercher un utilisateur' }), 'dav');
    await waitFor(() => expect(canvasElement.querySelectorAll('gbt-list-row')).toHaveLength(1));
    await expectUsersLayout(canvasElement);
  },
};

export const NoResults: Story = {
  play: async ({ canvasElement }) => {
    const page = within(canvasElement);
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await userEvent.type(page.getByRole('textbox', { name: 'Rechercher un utilisateur' }), 'zzz');
    await waitFor(() => expect(canvasElement.querySelector('[list-card-message]')).not.toBeNull());
  },
};

export const InviteDialog: Story = {
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await userEvent.click(await page.findByRole('button', { name: 'Inviter un utilisateur' }));
    await waitFor(() => expect(canvasElement.querySelector('fg-invite-user-modal [role="dialog"]')).not.toBeNull());
  },
};

export const PasswordResetConfirm: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'bob', 'Réinitialiser le mot de passe');
    await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
    await expect(canvasElement.querySelector('.gbt-confirm-danger-modal__message')?.textContent).toContain('cessera de fonctionner immédiatement');
  },
};

export const PasswordResetMailFailed: Story = {
  decorators: [withUsers(fakeUsersService(USERS, { mailFails: true }))],
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'bob', 'Réinitialiser le mot de passe');
    await userEvent.click(await page.findByRole('button', { name: 'Réinitialiser' }));
    await waitFor(() => expect(canvasElement.querySelector('.admin-users__mail-failed')).not.toBeNull(), { timeout: 3000 });
    await expect(canvasElement.querySelector('.admin-users__mail-failed')?.textContent).toContain('1 heure');
    await expectUsersLayout(canvasElement);
  },
};

export const DemoteConfirm: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'frank-admin', "Retirer les droits de super-administrateur");
    await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
  },
};

export const SelfDemote: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'alice', "Retirer les droits de super-administrateur");
    await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
    await expect(canvasElement.querySelector('.gbt-confirm-danger-modal__message')?.textContent).toContain('vous retirer vous-même');
  },
};

export const Promote: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await pickAction(canvasElement, 'carol', 'Nommer super-administrateur');
    await waitFor(
      () => {
        const row = Array.from(canvasElement.querySelectorAll('li[data-user-id="u3"] gbt-badge')).map((badge) => badge.textContent?.trim());
        expect(row).toContain('Super-administrateur');
      },
      { timeout: 3000 },
    );
    await expectUsersLayout(canvasElement);
  },
};

export const NameLinks: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull());
    await expect(canvasElement.querySelector('li[data-user-id="u2"] a.admin-users__user-link')?.getAttribute('href')).toBe('/admin/users/u2');
    await expect(canvasElement.querySelector('li[data-user-id="u1"] a')).toBeNull();
    await expect(canvasElement.querySelector('li[data-user-id="u2"] .gbt-menu__trigger')).not.toBeNull();
    await expectUsersLayout(canvasElement);
  },
};
