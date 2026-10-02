import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { delay, of, throwError } from 'rxjs';
import { InviteUserModal } from './invite-user-modal';
import { AdminUser, AdminUsersService, InviteResult } from '../../admin-users.service';
import { provideFerrisgitIcons } from '../../../shared/register-icons';

// provideFerrisgitIcons returns EnvironmentProviders: applicationConfig, not moduleMetadata.
const withIcons = applicationConfig({ providers: [provideFerrisgitIcons()] });

const ACTIVATION_URL = 'https://ferrisgit.example.com/activate#token=3f9c1a7be25d4c8e9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f708192a3b4c5';

const invitedUser = (username: string, email: string): AdminUser => ({
  id: 'new',
  username,
  email,
  isAdmin: false,
  createdAt: new Date().toISOString(),
  state: 'invited',
  invitationExpiresAt: new Date(Date.now() + 24 * 3_600_000).toISOString(),
  mfaEnabled: false,
});

const withInvite = (invite: AdminUsersService['invite']) => moduleMetadata({ providers: [{ provide: AdminUsersService, useValue: { invite } }] });

const sent: AdminUsersService['invite'] = (username, email) => of<InviteResult>({ user: invitedUser(username, email), emailSent: true }).pipe(delay(300));
const mailFails: AdminUsersService['invite'] = (username, email) =>
  of<InviteResult>({ user: invitedUser(username, email), emailSent: false, emailError: 'connection refused (smtp.mailgun.org:587)', activationUrl: ACTIVATION_URL }).pipe(delay(300));
const refused: AdminUsersService['invite'] = () => throwError(() => new HttpErrorResponse({ status: 409, error: { error: 'username already taken' } }));

const body = (canvasElement: HTMLElement) => within(canvasElement.ownerDocument.body);

async function fill(canvasElement: HTMLElement, username: string, email: string) {
  const page = body(canvasElement);
  await userEvent.type(await page.findByLabelText("Nom d'utilisateur"), username);
  await userEvent.type(page.getByLabelText('Adresse e-mail'), email);
}

const meta: Meta<InviteUserModal> = {
  title: 'Admin/InviteUserModal',
  component: InviteUserModal,
  tags: ['autodocs'],
  decorators: [withIcons, withInvite(sent)],
};

export default meta;
type Story = StoryObj<InviteUserModal>;

export const Form: Story = {};

export const Errors: Story = {
  play: async ({ canvasElement }) => {
    await fill(canvasElement, '1x', 'pas-une-adresse');
    await userEvent.click(body(canvasElement).getByRole('button', { name: "Envoyer l'invitation" }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('.gbt-input__error')).not.toBeNull());
  },
};

export const Refused: Story = {
  decorators: [withInvite(refused)],
  play: async ({ canvasElement }) => {
    await fill(canvasElement, 'bob', 'bob@ferrisgit.dev');
    await userEvent.click(body(canvasElement).getByRole('button', { name: "Envoyer l'invitation" }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('gbt-alert')).not.toBeNull());
  },
};

export const Sent: Story = {
  play: async ({ canvasElement }) => {
    await fill(canvasElement, 'bob', 'bob@ferrisgit.dev');
    await userEvent.click(body(canvasElement).getByRole('button', { name: "Envoyer l'invitation" }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('.invite-user__sent-title')).not.toBeNull());
  },
};

export const MailFailed: Story = {
  decorators: [withInvite(mailFails)],
  play: async ({ canvasElement }) => {
    await fill(canvasElement, 'bob', 'bob@ferrisgit.dev');
    await userEvent.click(body(canvasElement).getByRole('button', { name: "Envoyer l'invitation" }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('fg-link-mail-failed')).not.toBeNull());
  },
};
