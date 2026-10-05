import { HttpErrorResponse } from '@angular/common/http';
import { signal } from '@angular/core';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { delay, NEVER, Observable, of, throwError } from 'rxjs';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AdminUserDetail } from './admin-user-detail';
import { AdminUser, AdminUserRepository, AdminUsersService } from '../../admin-users.service';
import { MeService } from '../../../shell/me.service';
import { PageTitleService } from '../../../shell/page-title.service';
import { atPhoneWidth, daysAgo, hoursAgo, inShellContentArea, withFerrisgitIcons } from '../../../shared/layout/page-story-helpers';
import { fakeToast } from '../../../shared/layout/settings-story-helpers';

const inHours = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();

const user = (overrides: Partial<AdminUser> & Pick<AdminUser, 'id' | 'username'>): AdminUser => ({
  named: true,
  email: `${overrides.username}@ferrisgit.dev`,
  isAdmin: false,
  createdAt: daysAgo(45),
  state: 'active',
  invitationExpiresAt: null,
  mfaEnabled: true,
  ...overrides,
});

const USERS: AdminUser[] = [
  user({ id: 'u1', username: 'alice', isAdmin: true, createdAt: daysAgo(120) }),
  user({ id: 'u2', username: 'bob', email: 'bob.martin@example.org' }),
  user({ id: 'u3', username: 'carol', isAdmin: true, mfaEnabled: false, createdAt: daysAgo(6) }),
  user({ id: 'u4', username: 'dave', state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(19), createdAt: hoursAgo(5) }),
];

const MB = 1024 * 1024;
const REPOSITORIES: AdminUserRepository[] = [
  { id: 'r1', name: 'dotfiles', description: 'Configuration du poste : zsh, neovim, tmux.', visibility: 'public', createdAt: hoursAgo(3), sizeBytes: 820 * 1024 },
  { id: 'r2', name: 'rust-playground', description: '', visibility: 'private', createdAt: daysAgo(12), sizeBytes: 48 * MB },
  {
    id: 'r3',
    name: 'a-very-long-repository-name-that-keeps-going-and-going',
    description: 'Une description assez longue pour tenir sur plus d’une ligne à la largeur d’un téléphone, tronquée proprement.',
    visibility: 'private',
    createdAt: daysAgo(80),
    sizeBytes: 1.6 * 1024 * MB,
  },
  { id: 'r4', name: 'orphan', description: 'Son dossier a disparu du disque.', visibility: 'private', createdAt: daysAgo(200), sizeBytes: null },
];

interface FakeOptions {
  repositories?: AdminUserRepository[];
  refuseDelete?: string;
}

function fakeUsersService(options: FakeOptions = {}): Partial<AdminUsersService> {
  let current = [...USERS];
  return {
    list: () => of(current),
    repositories: () => of(options.repositories ?? REPOSITORIES).pipe(delay(300)),
    resend: (id) => of({ user: { ...current.find((u) => u.id === id)!, invitationExpiresAt: inHours(24) }, emailSent: true }).pipe(delay(600)),
    resetMfa: (id) => {
      current = current.map((u) => (u.id === id ? { ...u, mfaEnabled: false } : u));
      return of(undefined).pipe(delay(400));
    },
    resetPassword: () => of({ emailSent: true }).pipe(delay(400)),
    setAdmin: (id, isAdmin) => {
      current = current.map((u) => (u.id === id ? { ...u, isAdmin } : u));
      return of(undefined).pipe(delay(400));
    },
    deleteUser: (): Observable<void> =>
      options.refuseDelete ? throwError(() => new HttpErrorResponse({ status: 409, error: { error: options.refuseDelete } })) : of(undefined).pipe(delay(600)),
  };
}

const withService = (service: unknown) => moduleMetadata({ providers: [{ provide: AdminUsersService, useValue: service }] });
const withUserId = (id: string) => moduleMetadata({ providers: [{ provide: ActivatedRoute, useValue: { paramMap: of(convertToParamMap({ id })) } }] });

async function expectDetailLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-page-header, gbt-empty-state')) throw new Error('page not rendered yet');
  });
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) return; // hidden docs frame: no layout
  await expect(doc.scrollWidth, 'page scroll width').toBeLessThanOrEqual(doc.clientWidth);
  await expect(canvasElement.querySelector('.admin-user-detail__repositories a'), 'no link into a repository').toBeNull();
  const card = canvasElement.querySelector('gbt-list-card');
  if (card) {
    const cardRight = card.getBoundingClientRect().right;
    for (const row of Array.from(canvasElement.querySelectorAll<HTMLElement>('gbt-list-row'))) {
      await expect(row.getBoundingClientRect().right, 'row inside its card').toBeLessThanOrEqual(cardRight + 0.5);
      const name = row.querySelector<HTMLElement>('.admin-user-detail__repository-name')!;
      await expect(name.getBoundingClientRect().right, 'name inside its card').toBeLessThanOrEqual(cardRight + 0.5);
    }
  }
}

async function openDeletion(canvasElement: HTMLElement) {
  const page = within(canvasElement);
  await waitFor(() => expect(canvasElement.querySelector('gbt-list-row')).not.toBeNull(), { timeout: 3000 });
  const button = await page.findByRole('button', { name: "Supprimer l'utilisateur" });
  await waitFor(() => expect(button).not.toBeDisabled());
  await userEvent.click(button);
  await waitFor(() => expect(canvasElement.querySelector('gbt-confirm-danger-modal')).not.toBeNull());
}

async function confirmDeletion(canvasElement: HTMLElement) {
  await openDeletion(canvasElement);
  const body = within(canvasElement.ownerDocument.body);
  await userEvent.type(body.getByRole('textbox', { name: "Tapez le nom d'utilisateur pour confirmer" }), 'bob');
  const dialog = canvasElement.querySelector<HTMLElement>('gbt-confirm-danger-modal')!;
  await userEvent.click(within(dialog).getByRole('button', { name: "Supprimer l'utilisateur" }));
  await waitFor(() => expect(canvasElement.querySelector('.admin-user-detail__refusal-alert')).not.toBeNull(), { timeout: 3000 });
}

const meta: Meta<AdminUserDetail> = {
  title: 'Admin/AdminUserDetail',
  component: AdminUserDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    applicationConfig({ providers: [provideRouter([])] }),
    moduleMetadata({
      providers: [
        { provide: AdminUsersService, useValue: fakeUsersService() },
        { provide: ActivatedRoute, useValue: { paramMap: of(convertToParamMap({ id: 'u2' })) } },
        { provide: MeService, useValue: { id: signal('u1') } },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
  play: ({ canvasElement }) => expectDetailLayout(canvasElement),
};

export default meta;
type Story = StoryObj<AdminUserDetail>;

export const Default: Story = {};

export const SuperAdministrator: Story = {
  decorators: [withUserId('u3')],
};

export const Invited: Story = {
  decorators: [withUserId('u4'), withService(fakeUsersService({ repositories: [] }))],
};

export const NoRepositories: Story = {
  decorators: [withService(fakeUsersService({ repositories: [] }))],
};

export const DeleteConfirm: Story = {
  play: async ({ canvasElement }) => {
    await openDeletion(canvasElement);
    const message = canvasElement.querySelector('.gbt-confirm-danger-modal__message')?.textContent ?? '';
    await expect(message).toContain('Ses 4 dépôts personnels (au moins');
    await expect(message).toContain('Utilisateur supprimé');
    await expect(message).toContain('irréversible');
  },
};

export const DeleteRefusedLastAdmin: Story = {
  decorators: [withService(fakeUsersService({ refuseDelete: 'cannot remove the last administrator' }))],
  play: async ({ canvasElement }) => {
    await confirmDeletion(canvasElement);
    await expect(canvasElement.querySelector('.admin-user-detail__refusal-alert')?.textContent).toContain('dernier super-administrateur actif');
    await expectDetailLayout(canvasElement);
  },
};

export const DeleteRefusedLastMaintainer: Story = {
  decorators: [withService(fakeUsersService({ refuseDelete: 'the user is the last maintainer of the group acme/platform/infra; promote another member first' }))],
  play: async ({ canvasElement }) => {
    await confirmDeletion(canvasElement);
    await expect(canvasElement.querySelector('.admin-user-detail__refusal-alert')?.textContent).toContain('dernier mainteneur du groupe acme/platform/infra');
    await expectDetailLayout(canvasElement);
  },
};

export const Phone: Story = {
  decorators: [atPhoneWidth],
};

export const Loading: Story = {
  decorators: [withService({ list: () => NEVER, repositories: () => NEVER })],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('[aria-busy="true"]')).not.toBeNull());
  },
};

export const NotFound: Story = {
  decorators: [withUserId('nope')],
};

export const OwnAccount: Story = {
  decorators: [withUserId('u1')],
};
