import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { of } from 'rxjs';
import { Router } from '@angular/router';
import { PasskeySettings } from './passkey-settings';
import type { AuthConfig, Passkey } from '@masmarino/gabarit';
import { PASSWORD, StoryMfaStatus, withMfa } from '../mfa-story-helpers';
import { AuthService } from '../../auth/auth.service';
import { hoursAgo, daysAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, inSettingsColumn } from '../../shared/layout/settings-story-helpers';
import { dismissedPrompt, fakeAttestation, stubPasskeyBrowser } from '../../shared/webauthn-testing';

const MACBOOK: Passkey = { id: 'p1', name: 'MacBook Touch ID', createdAt: '2025-03-12T09:30:00Z', lastUsedAt: hoursAgo(3) };
const YUBIKEY: Passkey = { id: 'p2', name: 'YubiKey 5C NFC', createdAt: daysAgo(3), lastUsedAt: null };
const LONG_NAME: Passkey = {
  id: 'p3',
  name: "Clé d'accès du téléphone professionnel de l'équipe infrastructure",
  createdAt: daysAgo(40),
  lastUsedAt: daysAgo(1),
};

const withApp = (...passkeys: Passkey[]): StoryMfaStatus => ({ totpEnabled: true, backupCodesRemaining: 8, passkeys });
const passkeysOnly = (...passkeys: Passkey[]): StoryMfaStatus => ({ totpEnabled: false, backupCodesRemaining: 10, passkeys });

const passkeyBrowser = (create: (() => Promise<unknown>) | null) => () => stubPasskeyBrowser(create ? { create, get: () => Promise.reject(new Error('unused')) } : null);
const creates = () => Promise.resolve(fakeAttestation());

const withConfig = (config: AuthConfig) =>
  moduleMetadata({ providers: [{ provide: AuthService, useValue: { setToken: () => {}, logout: () => {}, authConfig: () => of(config) } }] });

const rect = (el: Element) => el.getBoundingClientRect();

async function expectPasskeyLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-card')) throw new Error('not rendered yet');
  });
  await expectSettingsLayout(canvasElement);
  await expect(canvasElement.querySelectorAll('.gbt-button--primary').length, 'primary buttons').toBeLessThanOrEqual(1);
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame
  }
  const body = canvasElement.querySelector('.gbt-card')!; // the box: a non-flush body is `display: contents`
  for (const row of Array.from(canvasElement.querySelectorAll<HTMLElement>('.gbt-passkey-settings__row'))) {
    const head = rect(row.querySelector('.gbt-passkey-settings__row-head')!);
    await expect(head.right, `row "${row.querySelector('.gbt-passkey-settings__name')?.textContent?.trim()}" inside the card`).toBeLessThanOrEqual(rect(body).right + 0.5);
    const name = row.querySelector<HTMLElement>('.gbt-passkey-settings__name')!;
    await expect(rect(name).height, 'the name is one line').toBeLessThanOrEqual(24.5);
  }
  if (doc.clientWidth <= 480) {
    for (const button of Array.from(canvasElement.querySelectorAll<HTMLElement>('button.gbt-button'))) {
      if (button.closest('gbt-confirm-danger-modal') || rect(button).width === 0) continue;
      await expect(rect(button).height, `"${button.textContent?.trim() || button.getAttribute('aria-label')}" is a 44px target`).toBeGreaterThanOrEqual(43.5);
    }
  }
}
const expectLayout = ({ canvasElement }: { canvasElement: HTMLElement }) => expectPasskeyLayout(canvasElement);

const buttonNamed = (canvas: ReturnType<typeof within>, name: string | RegExp) => canvas.findByRole('button', { name });

async function openAdd(canvas: ReturnType<typeof within>) {
  await userEvent.click(await buttonNamed(canvas, "Ajouter une clé d'accès"));
  return canvas.findByLabelText('Mot de passe actuel');
}

async function openDelete(canvas: ReturnType<typeof within>, name: string) {
  await userEvent.click(await buttonNamed(canvas, `Supprimer la clé ${name}`));
  return canvas.findByLabelText('Mot de passe actuel');
}

async function toDeleteDialog(canvasElement: HTMLElement, name: string, password = PASSWORD) {
  const canvas = within(canvasElement);
  await userEvent.type(await openDelete(canvas, name), password);
  await userEvent.click(await buttonNamed(canvas, 'Continuer'));
  return within(canvasElement.ownerDocument.body).findByRole('dialog');
}

const meta: Meta<PasskeySettings> = {
  title: 'Account/PasskeySettings',
  component: PasskeySettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  beforeEach: passkeyBrowser(creates),
  decorators: [
    withFerrisgitIcons,
    withMfa(withApp(MACBOOK, YUBIKEY)),
    withConfig({ registrationEnabled: false, passkeysAvailable: true }),
    moduleMetadata({
      providers: [
        // Deleting signs out and goes to /login, and the story has no router.
        { provide: Router, useValue: { navigateByUrl: () => Promise.resolve(true) } },
      ],
    }),
    inSettingsColumn,
  ],
};

export default meta;
type Story = StoryObj<PasskeySettings>;

export const Loading: Story = {
  decorators: [withMfa('loading')],
  play: expectLayout,
};

export const Failed: Story = {
  decorators: [withMfa('failed')],
  play: async (context) => {
    await within(context.canvasElement).findByRole('button', { name: 'Réessayer' });
    await expectLayout(context);
  },
};

export const List: Story = {
  play: async (context) => {
    await within(context.canvasElement).findByText('MacBook Touch ID');
    await expectLayout(context);
  },
};

export const LongName: Story = {
  decorators: [withMfa(withApp(LONG_NAME, YUBIKEY))],
  play: async (context) => {
    await within(context.canvasElement).findByText(/téléphone professionnel/);
    await expectLayout(context);
  },
};

export const Empty: Story = {
  decorators: [withMfa(withApp())],
  play: async (context) => {
    await within(context.canvasElement).findByText("Aucune clé d'accès");
    await expectLayout(context);
  },
};

export const Unsupported: Story = {
  beforeEach: passkeyBrowser(null),
  play: async (context) => {
    await within(context.canvasElement).findByText(/Ce navigateur ne prend pas en charge/);
    await expect(await buttonNamed(within(context.canvasElement), "Ajouter une clé d'accès")).toBeDisabled();
    await expectLayout(context);
  },
};

export const Unavailable: Story = {
  decorators: [withConfig({ registrationEnabled: false, passkeysAvailable: false }), withMfa(withApp())],
  play: async (context) => {
    await within(context.canvasElement).findByText(/ne sont pas disponibles sur ce serveur/);
    await expect(await buttonNamed(within(context.canvasElement), "Ajouter une clé d'accès")).toBeDisabled();
    await expectLayout(context);
  },
};

export const AddForm: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await buttonNamed(canvas, "Ajouter une clé d'accès"));
    await waitFor(() => expect(canvas.getByLabelText('Nom de la clé (facultatif)')).toHaveFocus());
    await expectLayout(context);
  },
};

export const AddFormFirstKey: Story = {
  decorators: [withMfa(withApp())],
  play: async (context) => {
    await openAdd(within(context.canvasElement));
    await expectLayout(context);
  },
};

export const Busy: Story = {
  beforeEach: passkeyBrowser(() => new Promise(() => {})),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openAdd(canvas), PASSWORD);
    await userEvent.type(canvas.getByLabelText('Nom de la clé (facultatif)'), 'Téléphone');
    await userEvent.click(await buttonNamed(canvas, "Créer la clé d'accès"));
    await canvas.findByText('Validez sur votre appareil pour créer la clé.');
    await expectLayout(context);
  },
};

export const WrongPassword: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openAdd(canvas), 'pas-le-bon');
    await userEvent.click(await buttonNamed(canvas, "Créer la clé d'accès"));
    await canvas.findByText('Mot de passe incorrect');
    await waitFor(() => expect(canvas.getByLabelText('Mot de passe actuel')).toHaveFocus());
    await expectLayout(context);
  },
};

export const AddCancelled: Story = {
  beforeEach: passkeyBrowser(() => Promise.reject(dismissedPrompt())),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openAdd(canvas), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, "Créer la clé d'accès"));
    await canvas.findByText('Opération annulée');
    await expectLayout(context);
  },
};

export const AlreadyRegistered: Story = {
  beforeEach: passkeyBrowser(() => Promise.reject(new DOMException('x', 'InvalidStateError'))),
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openAdd(canvas), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, "Créer la clé d'accès"));
    await canvas.findByText('Cette clé est déjà enregistrée');
    await expectLayout(context);
  },
};

export const Added: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openAdd(canvas), PASSWORD);
    await userEvent.type(canvas.getByLabelText('Nom de la clé (facultatif)'), 'Téléphone');
    await userEvent.click(await buttonNamed(canvas, "Créer la clé d'accès"));
    await canvas.findByText("Clé d'accès « Téléphone » ajoutée.", undefined, { timeout: 5000 });
    await expectLayout(context);
  },
};

export const DeletePrompt: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    const field = await openDelete(canvas, 'YubiKey 5C NFC');
    await waitFor(() => expect(field).toHaveFocus());
    await expectLayout(context);
  },
};

export const DeleteConfirm: Story = {
  play: async (context) => {
    const dialog = await toDeleteDialog(context.canvasElement, 'YubiKey 5C NFC');
    await expect(dialog).toHaveTextContent('Vous serez déconnecté de tous vos appareils');
    await expect(dialog).not.toHaveTextContent('dernier facteur');
    await expectLayout(context);
  },
};

export const Deleted: Story = {
  play: async (context) => {
    const dialog = await toDeleteDialog(context.canvasElement, 'YubiKey 5C NFC');
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Supprimer' }));
    await within(context.canvasElement).findByText('Vous avez été déconnecté. Reconnectez-vous pour continuer.');
    await expect(context.canvasElement.querySelectorAll('fg-passkey-settings button, fg-passkey-settings input')).toHaveLength(0);
    await expectLayout(context);
  },
};

export const LastFactorWarning: Story = {
  decorators: [withMfa(passkeysOnly(MACBOOK))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    const field = await openDelete(canvas, 'MacBook Touch ID');
    await expect(context.canvasElement.querySelector('gbt-alert')).toHaveTextContent("C'est votre dernier facteur");
    await expectLayout(context);
    await userEvent.type(field, PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Continuer'));
    const dialog = await within(context.canvasElement.ownerDocument.body).findByRole('dialog');
    await expect(dialog).toHaveTextContent("C'est votre dernier facteur");
  },
};

export const LastFactorPrompt: Story = {
  decorators: [withMfa(passkeysOnly(MACBOOK))],
  play: async (context) => {
    await openDelete(within(context.canvasElement), 'MacBook Touch ID');
    await expect(context.canvasElement.querySelector('gbt-alert')).toHaveTextContent("C'est votre dernier facteur");
    await expectLayout(context);
  },
};

export const AddHeldBack: Story = {
  decorators: [withMfa(withApp(MACBOOK), { appFlowActive: true })],
  play: async (context) => {
    await within(context.canvasElement).findByText(/Terminez d'abord la configuration/);
    await expect(await buttonNamed(within(context.canvasElement), "Ajouter une clé d'accès")).toBeDisabled();
    await expectLayout(context);
  },
};

export const DeleteWrongPassword: Story = {
  play: async (context) => {
    const dialog = await toDeleteDialog(context.canvasElement, 'YubiKey 5C NFC', 'pas-le-bon');
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Supprimer' }));
    await within(context.canvasElement).findByText('Mot de passe incorrect');
    await expectLayout(context);
  },
};
