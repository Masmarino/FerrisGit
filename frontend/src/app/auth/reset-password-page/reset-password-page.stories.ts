import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { ActivatedRoute, convertToParamMap, provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ResetPasswordPage } from './reset-password-page';
import { AuthService } from '../auth.service';
import { provideFerrisgitAuth } from '../auth-kit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { expectPanelLayout } from '../auth-story-helpers';

const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const withLink = (token: string | null) =>
  moduleMetadata({ providers: [{ provide: ActivatedRoute, useValue: { snapshot: { fragment: token === null ? null : `token=${token}`, queryParamMap: convertToParamMap({}) } } }] });
const withServer = (reset$: Observable<void>) => moduleMetadata({ providers: [{ provide: AuthService, useValue: { resetPassword: () => reset$ } satisfies Partial<AuthService> }] });

const LINK = withLink('ab12'.repeat(16));
const failure = (status: number, error: string) => throwError(() => new HttpErrorResponse({ status, statusText: 'x', error: { error } }));
const expectLayout = expectPanelLayout();

async function fillAndSubmit(canvasElement: HTMLElement, confirmation = 'correct-horse-battery') {
  const canvas = within(canvasElement);
  await userEvent.type(await canvas.findByLabelText('Nouveau mot de passe'), 'correct-horse-battery');
  await userEvent.type(canvas.getByLabelText('Confirmez le nouveau mot de passe'), confirmation);
  await userEvent.click(canvas.getByRole('button', { name: 'Enregistrer le mot de passe' }));
  return canvas;
}

const meta: Meta<ResetPasswordPage> = {
  title: 'Auth/ResetPasswordPage',
  component: ResetPasswordPage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withApp, moduleMetadata({ providers: provideFerrisgitAuth() })],
};

export default meta;
type Story = StoryObj<ResetPasswordPage>;

export const Default: Story = {
  decorators: [LINK, withServer(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await waitFor(() => expect(canvas.getByLabelText('Nouveau mot de passe')).toHaveFocus());
    await expect(canvas.getByText('Au moins 8 caractères.')).toBeVisible();
    await expect(canvas.getByText(/votre ancien mot de passe ne fonctionne plus/)).toBeVisible();
    await expectLayout(context);
  },
};

export const Mismatch: Story = {
  decorators: [LINK, withServer(NEVER)],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement, 'correct-horse-batterz');
    await expect(await canvas.findByText('Les mots de passe ne correspondent pas')).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText('Confirmez le nouveau mot de passe')).toHaveFocus());
    await expectLayout(context);
  },
};

export const Success: Story = {
  decorators: [LINK, withServer(of(undefined))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Votre mot de passe a été modifié' })).toBeVisible();
    await waitFor(() => expect(canvas.getByRole('heading', { name: 'Votre mot de passe a été modifié' })).toHaveFocus());
    await expect(canvas.getByRole('button', { name: 'Se connecter' })).toBeVisible();
    await expectLayout(context);
  },
};

export const InvalidLink: Story = {
  decorators: [withLink(null), withServer(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Ce lien ne fonctionne pas' })).toBeVisible();
    await expect(canvas.getByText(/Ce lien de réinitialisation est invalide ou a expiré/)).toBeVisible();
    await expect(canvas.queryByLabelText('Nouveau mot de passe')).toBeNull();
    await expectLayout(context);
  },
};

export const ExpiredLink: Story = {
  decorators: [LINK, withServer(failure(400, 'invalid or expired password reset link'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Ce lien ne fonctionne pas' })).toBeVisible();
    await expectLayout(context);
  },
};

export const RateLimited: Story = {
  decorators: [LINK, withServer(failure(429, 'too many password reset attempts, try again later'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Trop de tentatives, réessayez dans quelques minutes');
    await expectLayout(context);
  },
};

export const Submitting: Story = {
  decorators: [LINK, withServer(NEVER)],
  play: async (context) => {
    await fillAndSubmit(context.canvasElement);
    await waitFor(() => expect(context.canvasElement.querySelector('.gbt-auth-panel__submit button')).toBeDisabled());
    await expectLayout(context);
  },
};
