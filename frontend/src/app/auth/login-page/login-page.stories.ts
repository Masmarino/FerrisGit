import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { LoginPage } from './login-page';
import type { LoginResponse } from '@masmarino/gabarit/auth';
import { AuthService } from '../auth.service';
import { provideFerrisgitAuth } from '../auth-kit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { REQUEST_OPTIONS, dismissedPrompt, fakeAssertion, stubPasskeyBrowser } from '../../shared/webauthn-testing';

// Environment providers only work in `applicationConfig`, not `moduleMetadata`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

function withLogin(login$: Observable<LoginResponse>, more: Partial<AuthService> = {}) {
  return moduleMetadata({ providers: [{ provide: AuthService, useValue: { login: () => login$, authConfig: () => of({ registrationEnabled: false, passkeysAvailable: false }), ...more } satisfies Partial<AuthService> }] });
}

const CHALLENGE: LoginResponse = { token: null, mfaToken: 'pending', mfaSetupRequired: false, mfaHasTotp: true, mfaHasPasskey: false };
const SETUP: LoginResponse = { token: null, mfaToken: 'pending', mfaSetupRequired: true, mfaHasTotp: false, mfaHasPasskey: false };
const PASSKEY: LoginResponse = { token: null, mfaToken: 'pending', mfaSetupRequired: false, mfaHasTotp: false, mfaHasPasskey: true };
const BOTH: LoginResponse = { token: null, mfaToken: 'pending', mfaSetupRequired: false, mfaHasTotp: true, mfaHasPasskey: true };
const status = (code: number) => throwError(() => new HttpErrorResponse({ status: code }));

const PASSKEY_SERVER: Partial<AuthService> = {
  authConfig: () => of({ registrationEnabled: false, passkeysAvailable: true }),
  startPasskeyChallenge: () => of({ challengeId: '7b1f7f2e-0000-4000-8000-000000000001', publicKey: REQUEST_OPTIONS }),
  finishPasskeyChallenge: () => of(undefined),
};

// Stubs WebAuthn for the story and restores it afterwards.
const passkeyBrowser = (get: (() => Promise<unknown>) | null) => () => stubPasskeyBrowser(get ? { get, create: () => new Promise(() => undefined) } : null);
const answers = () => Promise.resolve(fakeAssertion());
const dismisses = () => Promise.reject(dismissedPrompt());

const rect = (el: Element) => el.getBoundingClientRect();

function assertLayout(canvas: HTMLElement): void {
  const panel = canvas.querySelector('.gbt-auth-panel__panel');
  const img = canvas.querySelector<HTMLImageElement>('.gbt-auth-panel__logo img');
  if (!panel || !img?.complete) throw new Error('page not rendered yet');
  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  if (rect(panel).left < 15.5 || rect(panel).right > doc.clientWidth - 15.5) throw new Error('the panel has no 16px gutter');
  for (const part of Array.from(canvas.querySelectorAll('.gbt-auth-panel__logo img, .gbt-auth-panel__form .gbt-input input, .gbt-auth-panel__submit button'))) {
    if (rect(part).left < rect(panel).left || rect(part).right > rect(panel).right) throw new Error('content spills out of the panel');
  }
  const button = canvas.querySelector('.gbt-auth-panel__submit button')!;
  const field = canvas.querySelector('.gbt-auth-panel__form .gbt-input input')!;
  if (Math.abs(rect(button).width - rect(field.closest('.gbt-input')!).width) > 1) throw new Error('the button should span the form');
}

function assertChallengeLayout(canvas: HTMLElement): void {
  const panel = canvas.querySelector('.gbt-auth-panel__panel');
  if (!panel) throw new Error('page not rendered yet');
  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  if (rect(panel).left < 15.5 || rect(panel).right > doc.clientWidth - 15.5) throw new Error('the panel has no 16px gutter');
  for (const part of Array.from(canvas.querySelectorAll('.gbt-auth-panel__panel gbt-input input, .gbt-auth-panel__panel button, .gbt-auth-panel__panel .gbt-alert'))) {
    if (rect(part).left < rect(panel).left || rect(part).right > rect(panel).right) throw new Error(`content spills out of the panel: ${part.className}`);
  }
  for (const tap of Array.from(canvas.querySelectorAll('.gbt-auth-panel__panel a.gbt-button, .gbt-auth-panel__panel button.gbt-button'))) {
    if (rect(tap).height < 43.5) throw new Error(`tap target under 44px: ${tap.className} ${rect(tap).height}px`);
  }
  for (const divider of Array.from(canvas.querySelectorAll('gbt-divider'))) {
    if (rect(divider).left < rect(panel).left || rect(divider).right > rect(panel).right) throw new Error('the divider spills out of the panel');
  }
}

async function expectLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertLayout(canvasElement), { timeout: 3000 });
}

const meta: Meta<LoginPage> = {
  title: 'Auth/LoginPage',
  component: LoginPage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withApp, moduleMetadata({ providers: provideFerrisgitAuth() })],
};

export default meta;
type Story = StoryObj<LoginPage>;

export const Default: Story = {
  decorators: [withLogin(NEVER)],
  play: expectLayout,
};

export const WithRegistrationLink: Story = {
  decorators: [withLogin(NEVER, { authConfig: () => of({ registrationEnabled: true, passkeysAvailable: false }) })],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    const link = await canvas.findByRole('link', { name: 'Créer un compte' });
    await expect(link.getAttribute('href')).toMatch(/\/register$/);
    await waitFor(() => {
      if (rect(link).height < 43.5) throw new Error(`tap target under 44px: ${rect(link).height}px`);
      const panel = context.canvasElement.querySelector('.gbt-auth-panel__panel')!;
      if (rect(link).right > rect(panel).right || rect(link).left < rect(panel).left) throw new Error('the link spills out of the panel');
    });
    await expectLayout(context);
  },
};

export const Challenge: Story = {
  decorators: [withLogin(of(CHALLENGE))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('heading', { name: 'Vérification en deux étapes' })).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText('Code à 6 chiffres')).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const ChallengeBackupCode: Story = {
  decorators: [withLogin(of(CHALLENGE))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await userEvent.click(await canvas.findByRole('button', { name: 'Utiliser un code de secours' }));
    await waitFor(() => expect(canvas.getByLabelText('Code de secours')).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const ChallengeError: Story = {
  decorators: [withLogin(of(CHALLENGE), { verifyMfa: () => status(401) })],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await userEvent.type(await canvas.findByLabelText('Code à 6 chiffres'), '000000');
    await userEvent.click(canvas.getByRole('button', { name: 'Vérifier' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Code incorrect');
    await waitFor(() => expect(canvas.getByLabelText('Code à 6 chiffres')).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const ChallengeExpired: Story = {
  decorators: [
    withLogin(of(CHALLENGE), {
      verifyMfa: () => throwError(() => new HttpErrorResponse({ status: 401, error: { error: 'invalid or expired token' } })),
    }),
  ],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await userEvent.type(await canvas.findByLabelText('Code à 6 chiffres'), '123456');
    await userEvent.click(canvas.getByRole('button', { name: 'Vérifier' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Votre connexion a expiré, reconnectez-vous.');
    await waitFor(() => expect(canvas.getByLabelText('Mot de passe')).toHaveFocus());
    await expectLayout(context);
  },
};

export const ChallengePasskey: Story = {
  decorators: [withLogin(of(PASSKEY), PASSKEY_SERVER)],
  beforeEach: passkeyBrowser(answers),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('heading', { name: 'Vérification en deux étapes' })).toBeVisible();
    await waitFor(() => expect(canvas.getByRole('button', { name: "Utiliser une clé d'accès" })).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const ChallengeBoth: Story = {
  decorators: [withLogin(of(BOTH), PASSKEY_SERVER)],
  beforeEach: passkeyBrowser(answers),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await canvas.findByLabelText('Code à 6 chiffres');
    await expect(canvas.getByRole('button', { name: "Utiliser une clé d'accès" })).toHaveClass('gbt-button--primary');
    await expect(canvas.getByRole('button', { name: 'Vérifier' })).toHaveClass('gbt-button--secondary');
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const ChallengePasskeyUnsupported: Story = {
  decorators: [withLogin(of(PASSKEY), PASSKEY_SERVER)],
  beforeEach: passkeyBrowser(null),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByText("Ce navigateur ne prend pas en charge les clés d'accès. Utilisez un code de secours.")).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText('Code de secours')).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const PasskeyCancelled: Story = {
  decorators: [withLogin(of(PASSKEY), PASSKEY_SERVER)],
  beforeEach: passkeyBrowser(dismisses),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await userEvent.click(await canvas.findByRole('button', { name: "Utiliser une clé d'accès" }));
    await expect(await canvas.findByText('Opération annulée')).toBeVisible();
    await waitFor(() => expect(canvas.getByRole('button', { name: "Utiliser une clé d'accès" })).toHaveFocus());
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const PasskeyRefused: Story = {
  decorators: [withLogin(of(PASSKEY), { ...PASSKEY_SERVER, finishPasskeyChallenge: () => status(401) })],
  beforeEach: passkeyBrowser(answers),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await userEvent.click(await canvas.findByRole('button', { name: "Utiliser une clé d'accès" }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent("Clé d'accès refusée");
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const PasskeyPrompt: Story = {
  decorators: [withLogin(of(PASSKEY), PASSKEY_SERVER)],
  beforeEach: passkeyBrowser(() => new Promise(() => undefined)),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    // Ids are generated per instance, so find the button by name.
    const passkey = await canvas.findByRole('button', { name: "Utiliser une clé d'accès" });
    await userEvent.click(passkey);
    await waitFor(() => expect(passkey).toHaveAttribute('aria-busy', 'true'));
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const Enrollment: Story = {
  decorators: [withLogin(of(SETUP))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('heading', { name: 'Protégez votre compte' })).toBeVisible();
    await waitFor(() => assertChallengeLayout(context.canvasElement), { timeout: 3000 });
  },
};

export const LoginFailure: Story = {
  decorators: [withLogin(throwError(() => new HttpErrorResponse({ status: 401, statusText: 'Unauthorized', error: { error: 'invalid username or password' } })))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent("Nom d'utilisateur ou mot de passe incorrect");
    await expectLayout(context);
  },
};

export const LoginServerError: Story = {
  decorators: [withLogin(throwError(() => new HttpErrorResponse({ status: 500, statusText: 'Server Error', error: { error: 'internal error' } })))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent('La connexion a échoué, réessayez.');
    await expectLayout(context);
  },
};

export const LoginRateLimited: Story = {
  decorators: [withLogin(throwError(() => new HttpErrorResponse({ status: 429, statusText: 'Too Many Requests', error: { error: 'too many attempts, try again later' } })))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await expect(await canvas.findByRole('alert')).toHaveTextContent('Trop de tentatives, réessayez dans quelques minutes');
    await expectLayout(context);
  },
};

export const Submitting: Story = {
  decorators: [withLogin(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Se connecter' }));
    await waitFor(() => expect(context.canvasElement.querySelector('.gbt-auth-panel__submit button')).toBeDisabled());
    await expectLayout(context);
  },
};
