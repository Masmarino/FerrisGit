import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { RegisterPage } from './register-page';
import type { AuthConfig } from '@masmarino/gabarit/auth';
import { AuthService } from '../auth.service';
import { provideFerrisgitAuth } from '../auth-kit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { expectPanelLayout } from '../auth-story-helpers';

// Environment providers only work in `applicationConfig`, not `moduleMetadata`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const failure = (status: number, error: string) => throwError(() => new HttpErrorResponse({ status, statusText: 'x', error: { error } }));

function withServer(register$: Observable<void>, config$: Observable<AuthConfig> = of({ registrationEnabled: true, passkeysAvailable: false })) {
  const fake = { authConfig: () => config$, register: () => register$ } satisfies Partial<AuthService>;
  return moduleMetadata({ providers: [{ provide: AuthService, useValue: fake }] });
}

const expectLayout = expectPanelLayout();

async function fillAndSubmit(canvasElement: HTMLElement, username = 'alice') {
  const canvas = within(canvasElement);
  await userEvent.type(await canvas.findByLabelText("Nom d'utilisateur"), username);
  await userEvent.type(canvas.getByLabelText('Adresse e-mail'), 'alice@example.com');
  await userEvent.click(canvas.getByRole('button', { name: 'Créer mon compte' }));
  return canvas;
}

const meta: Meta<RegisterPage> = {
  title: 'Auth/RegisterPage',
  component: RegisterPage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withApp, moduleMetadata({ providers: provideFerrisgitAuth() })],
};

export default meta;
type Story = StoryObj<RegisterPage>;

export const Default: Story = {
  decorators: [withServer(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expect(canvas.getByText('3 à 32 caractères, lettres, chiffres, - et _. Enregistré en minuscules.')).toBeVisible();
    await expect(canvas.getByText("Nous vous y envoyons un lien pour confirmer l'adresse et choisir votre mot de passe.")).toBeVisible();
    await expect(canvas.queryByLabelText('Mot de passe')).toBeNull();
    await expect(canvas.getByRole('link', { name: 'Se connecter' }).getAttribute('href')).toMatch(/\/login$/);
    await expectLayout(context);
  },
};

export const Loading: Story = {
  decorators: [withServer(NEVER, NEVER)],
  play: async (context) => {
    // Announced once, and not from inside the hidden skeleton: an aria-hidden ancestor would silence the live region.
    const status = within(context.canvasElement).getByRole('status');
    await expect(status).toHaveTextContent('Chargement en cours');
    await expect(status.closest('[aria-hidden="true"]')).toBeNull();
    await expect(context.canvasElement.querySelector('form')).toBeNull();
    await expectLayout(context);
  },
};

export const FieldErrors: Story = {
  decorators: [withServer(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await canvas.findByLabelText("Nom d'utilisateur"), '1a');
    await userEvent.type(canvas.getByLabelText('Adresse e-mail'), 'nope');
    await userEvent.click(canvas.getByRole('button', { name: 'Créer mon compte' }));
    await expect(await canvas.findByText('Commencez par une lettre ; 3 à 32 caractères : lettres, chiffres, - et _')).toBeVisible();
    await expect(canvas.getByText('Saisissez une adresse e-mail valide, par exemple nom@exemple.fr')).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expectLayout(context);
  },
};

export const Taken: Story = {
  decorators: [withServer(failure(409, 'username already taken'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent("Ce nom d'utilisateur ou cette adresse e-mail est déjà utilisé");
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expectLayout(context);
  },
};

export const RateLimited: Story = {
  decorators: [withServer(failure(429, 'too many registration attempts, try again later'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Trop de tentatives, réessayez dans quelques minutes');
    await expectLayout(context);
  },
};

export const MailFailed: Story = {
  decorators: [withServer(failure(503, 'the confirmation e-mail could not be sent, try again later'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent("Le message de confirmation n'a pas pu être envoyé");
    await expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveValue('alice');
    await expectLayout(context);
  },
};

export const ServerError: Story = {
  decorators: [withServer(failure(500, 'internal error'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent("L'inscription a échoué, réessayez.");
    await expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveValue('alice');
    await expectLayout(context);
  },
};

export const Submitting: Story = {
  decorators: [withServer(NEVER)],
  play: async (context) => {
    await fillAndSubmit(context.canvasElement);
    await waitFor(() => expect(context.canvasElement.querySelector('.gbt-auth-panel__submit button')).toBeDisabled());
    await expectLayout(context);
  },
};

export const Closed: Story = {
  decorators: [withServer(NEVER, of({ registrationEnabled: false, passkeysAvailable: false }))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Les inscriptions sont fermées' })).toBeVisible();
    await waitFor(() => expect(canvas.getByRole('heading', { name: 'Les inscriptions sont fermées' })).toHaveFocus());
    await expect(canvas.queryByLabelText("Nom d'utilisateur")).toBeNull();
    await expectLayout(context);
  },
};

export const Sent: Story = {
  decorators: [withServer(of(undefined))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement, 'Alice');
    const heading = await canvas.findByRole('heading', { name: 'Consultez votre boîte mail' });
    await expect(heading).toBeVisible();
    await waitFor(() => expect(heading).toHaveFocus());
    await expect(canvas.getByText(/alice@example\.com/)).toBeVisible();
    await expect(canvas.getByText(/Inscrivez-vous de nouveau/)).toBeVisible();
    await expect(canvas.getByRole('button', { name: 'Se connecter' })).toBeVisible();
    await expect(canvas.queryByLabelText("Nom d'utilisateur")).toBeNull();
    await expectLayout(context);
  },
};
