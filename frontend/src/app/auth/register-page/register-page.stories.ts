import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { RegisterPage } from './register-page';
import type { AuthConfig, LoginResponse } from '@masmarino/gabarit';
import { AuthService } from '../auth.service';
import { provideFerrisgitAuth } from '../auth-kit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { stubPasskeyBrowser } from '../../shared/webauthn-testing';
import { expectPanelLayout } from '../auth-story-helpers';

// Environment providers only work in `applicationConfig`, not `moduleMetadata`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const OTPAUTH_URL = 'otpauth://totp/FerrisGit:alice?secret=JBSWY3DPEHPK3PXP&issuer=FerrisGit';
const PENDING: LoginResponse = { token: null, mfaToken: 'pending', mfaSetupRequired: true, mfaHasTotp: false, mfaHasPasskey: false };
const failure = (status: number, error: string) => throwError(() => new HttpErrorResponse({ status, statusText: 'x', error: { error } }));

function withServer(register$: Observable<LoginResponse>, registrationEnabled$: Observable<AuthConfig> = of({ registrationEnabled: true, passkeysAvailable: false })) {
  const fake = {
    authConfig: () => registrationEnabled$,
    register: () => register$,
    enrollTotp: () => of({ secret: 'JBSWY3DPEHPK3PXP', otpauthUrl: OTPAUTH_URL }),
  } satisfies Partial<AuthService>;
  return moduleMetadata({ providers: [{ provide: AuthService, useValue: fake }] });
}

const expectLayout = expectPanelLayout();

async function fillAndSubmit(canvasElement: HTMLElement, username = 'alice') {
  const canvas = within(canvasElement);
  await userEvent.type(await canvas.findByLabelText("Nom d'utilisateur"), username);
  await userEvent.type(canvas.getByLabelText('Adresse e-mail'), 'alice@example.com');
  await userEvent.type(canvas.getByLabelText('Mot de passe'), 'correct-horse-battery');
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
    await expect(canvas.getByRole('link', { name: 'Se connecter' }).getAttribute('href')).toMatch(/\/login$/);
    await expectLayout(context);
  },
};

export const Loading: Story = {
  decorators: [withServer(NEVER, NEVER)],
  play: async (context) => {
    // Announced once, and not from inside the busy skeleton: a busy ancestor keeps a live region quiet.
    const status = within(context.canvasElement).getByRole('status');
    await expect(status).toHaveTextContent('Chargement en cours');
    await expect(status.closest('[aria-busy="true"]')).toBeNull();
    await expect(context.canvasElement.querySelector('.gbt-auth-register__loading[aria-busy="true"]')).not.toBeNull();
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
    await userEvent.type(canvas.getByLabelText('Mot de passe'), 'short');
    await userEvent.click(canvas.getByRole('button', { name: 'Créer mon compte' }));
    await expect(await canvas.findByText('Commencez par une lettre ; 3 à 32 caractères : lettres, chiffres, - et _')).toBeVisible();
    await expect(canvas.getByText('Saisissez une adresse e-mail valide, par exemple nom@exemple.fr')).toBeVisible();
    await expect(canvas.getByText('Au moins 8 caractères')).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expectLayout(context);
  },
};

export const Errors: Story = {
  decorators: [withServer(failure(409, 'username already taken'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent("Ce nom d'utilisateur ou cette adresse e-mail est déjà utilisé");
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expectLayout(context);
  },
};

export const RateLimited: Story = {
  decorators: [withServer(failure(429, 'too many attempts, try again later'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Trop de tentatives, réessayez dans quelques minutes');
    await expectLayout(context);
  },
};

export const ServerError: Story = {
  decorators: [withServer(failure(500, 'internal error'))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent('La création du compte a échoué, réessayez.');
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

export const Disabled: Story = {
  decorators: [withServer(NEVER, of({ registrationEnabled: false, passkeysAvailable: false }))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Les inscriptions sont fermées' })).toBeVisible();
    await waitFor(() => expect(canvas.getByRole('heading', { name: 'Les inscriptions sont fermées' })).toHaveFocus());
    await expect(canvas.queryByLabelText("Nom d'utilisateur")).toBeNull();
    await expectLayout(context);
  },
};

export const Enrollment: Story = {
  decorators: [withServer(of(PENDING))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Protégez votre compte' })).toBeVisible();
    await expect(canvas.getByRole('heading', { level: 1, name: 'Double authentification' })).toBeVisible();
    await expectLayout(context);
  },
};

// Stubs WebAuthn whatever browser runs Storybook, and restores it afterwards.
const passkeyBrowser = () => stubPasskeyBrowser({ create: () => new Promise(() => undefined), get: () => new Promise(() => undefined) });

export const EnrollmentWithPasskeys: Story = {
  decorators: [withServer(of(PENDING), of({ registrationEnabled: true, passkeysAvailable: true }))],
  beforeEach: passkeyBrowser,
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('button', { name: "Utiliser une clé d'accès" })).toHaveClass('gbt-button--primary');
    await expect(canvas.getByRole('button', { name: 'Utiliser une application' })).toBeVisible();
    await expectLayout(context);
  },
};

export const EnrollmentWithoutPasskeys: Story = {
  decorators: [withServer(of(PENDING), of({ registrationEnabled: true, passkeysAvailable: false }))],
  beforeEach: passkeyBrowser,
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('button', { name: 'Commencer' })).toHaveClass('gbt-button--primary');
    await expect(canvas.queryByRole('button', { name: "Utiliser une clé d'accès" })).toBeNull();
    await expectLayout(context);
  },
};

export const Created: Story = {
  decorators: [withServer(of(PENDING))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement, 'Alice');
    await userEvent.click(await canvas.findByRole('button', { name: 'Retour' }));
    await expect(await canvas.findByRole('heading', { name: 'Votre compte est créé' })).toBeVisible();
    await expect(context.canvasElement.querySelector('.gbt-auth-register__created-name')).toHaveTextContent("Votre nom d'utilisateur : alice");
    await waitFor(() => expect(canvas.getByRole('heading', { name: 'Votre compte est créé' })).toHaveFocus());
    await expect(canvas.getByRole('button', { name: 'Se connecter' })).toBeVisible();
    await expectLayout(context);
  },
};
