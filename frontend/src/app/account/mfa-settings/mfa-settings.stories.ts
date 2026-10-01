import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { Router } from '@angular/router';
import { MfaSettings } from './mfa-settings';
import type { MfaStatus } from '@masmarino/gabarit';
import { PASSWORD, CODE, withMfa } from '../mfa-story-helpers';
import { AuthService } from '../../auth/auth.service';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, inSettingsColumn } from '../../shared/layout/settings-story-helpers';

const ENABLED: MfaStatus = { totpEnabled: true, backupCodesRemaining: 8, passkeys: [] };
const DISABLED: MfaStatus = { totpEnabled: false, backupCodesRemaining: 0, passkeys: [] };
const PASSKEY_ONLY: MfaStatus = { totpEnabled: false, backupCodesRemaining: 10, passkeys: [{ id: 'p1', name: 'MacBook Touch ID', createdAt: '2025-03-12T09:30:00Z', lastUsedAt: null }] };
const APP_AND_PASSKEY: MfaStatus = { totpEnabled: true, backupCodesRemaining: 8, passkeys: PASSKEY_ONLY.passkeys };

const rect = (el: Element) => el.getBoundingClientRect();

async function expectMfaLayout(canvasElement: HTMLElement) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-card')) throw new Error('not rendered yet');
  });
  await expectSettingsLayout(canvasElement);
  await expect(canvasElement.querySelectorAll('.gbt-button--primary').length, 'primary buttons').toBeLessThanOrEqual(1);
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame
  }
  if (doc.clientWidth <= 480) {
    for (const button of Array.from(canvasElement.querySelectorAll<HTMLElement>('button.gbt-button'))) {
      if (button.closest('gbt-backup-codes, gbt-totp-qr, gbt-confirm-danger-modal') || rect(button).width === 0) continue;
      await expect(rect(button).height, `"${button.textContent?.trim()}" is a 44px target`).toBeGreaterThanOrEqual(43.5);
    }
  }
}
const expectLayout = ({ canvasElement }: { canvasElement: HTMLElement }) => expectMfaLayout(canvasElement);

const buttonNamed = (canvas: ReturnType<typeof within>, name: string) => canvas.findByRole('button', { name });

async function openPrompt(canvas: ReturnType<typeof within>, opener: string) {
  await userEvent.click(await buttonNamed(canvas, opener));
  return canvas.findByLabelText('Mot de passe actuel');
}

async function toScan(canvasElement: HTMLElement) {
  const canvas = within(canvasElement);
  await userEvent.type(await openPrompt(canvas, 'Configurer maintenant'), PASSWORD);
  await userEvent.click(await buttonNamed(canvas, 'Continuer'));
  await waitFor(() => expect(canvasElement.querySelector<HTMLImageElement>('.gbt-totp-qr__frame img')?.src).toMatch(/^data:image\/png;base64,/), { timeout: 5000 });
  return canvas;
}

async function toCodes(canvasElement: HTMLElement) {
  const canvas = await toScan(canvasElement);
  await userEvent.type(await canvas.findByLabelText('Code à 6 chiffres'), CODE);
  await userEvent.click(await buttonNamed(canvas, 'Activer'));
  await canvas.findByRole('button', { name: 'Copier les codes' });
  return canvas;
}

const meta: Meta<MfaSettings> = {
  title: 'Account/MfaSettings',
  component: MfaSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withMfa(ENABLED),
    moduleMetadata({
      providers: [
        { provide: AuthService, useValue: { setToken: () => {}, logout: () => {} } },
        // The reset signs out and goes to /login, and the story has no router.
        { provide: Router, useValue: { navigateByUrl: () => Promise.resolve(true) } },
      ],
    }),
    inSettingsColumn,
  ],
};

export default meta;
type Story = StoryObj<MfaSettings>;

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

export const Enabled: Story = {
  play: async (context) => {
    await within(context.canvasElement).findByText('Activée');
    await expectLayout(context);
  },
};

export const FewCodesLeft: Story = {
  decorators: [withMfa({ ...ENABLED, backupCodesRemaining: 2 })],
  play: async (context) => {
    await within(context.canvasElement).findByText(/Régénérez-en/);
    await expectLayout(context);
  },
};

export const NoCodesLeft: Story = {
  decorators: [withMfa({ ...ENABLED, backupCodesRemaining: 0 })],
  play: async (context) => {
    await within(context.canvasElement).findByText(/Aucun code de secours restant/);
    await expectLayout(context);
  },
};

export const RegeneratePrompt: Story = {
  play: async (context) => {
    const field = await openPrompt(within(context.canvasElement), 'Régénérer les codes de secours');
    await waitFor(() => expect(field).toHaveFocus());
    await expectLayout(context);
  },
};

export const RegenerateWrongPassword: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Régénérer les codes de secours'), 'pas-le-bon');
    await userEvent.click(await buttonNamed(canvas, 'Régénérer'));
    await canvas.findByText('Mot de passe incorrect');
    await waitFor(() => expect(canvas.getByLabelText('Mot de passe actuel')).toHaveFocus());
    await expectLayout(context);
  },
};

export const RegeneratedCodes: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Régénérer les codes de secours'), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Régénérer'));
    await canvas.findByRole('button', { name: 'Copier les codes' });
    await expect(canvas.getByRole('button', { name: 'Terminé' })).toBeDisabled();
    await expectLayout(context);
  },
};

export const RegeneratedCodesAcknowledged: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Régénérer les codes de secours'), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Régénérer'));
    await userEvent.click(await canvas.findByLabelText("J'ai enregistré mes codes de secours"));
    await waitFor(() => expect(canvas.getByRole('button', { name: 'Terminé' })).toBeEnabled());
    await expectLayout(context);
  },
};

export const DisablePrompt: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Réinitialiser'), PASSWORD);
    await expectLayout(context);
  },
};

export const DisableConfirmation: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Réinitialiser'), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Continuer'));
    const dialog = await within(context.canvasElement.ownerDocument.body).findByRole('dialog');
    await expect(dialog).toHaveTextContent('Réinitialiser la double authentification ?');
    await expectLayout(context);
  },
};

export const DisableWrongPassword: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Réinitialiser'), 'pas-le-bon');
    await userEvent.click(await buttonNamed(canvas, 'Continuer'));
    const body = within(context.canvasElement.ownerDocument.body);
    await userEvent.click(await within(await body.findByRole('dialog')).findByRole('button', { name: 'Réinitialiser' }));
    await canvas.findByText('Mot de passe incorrect');
    await expectLayout(context);
  },
};

export const Revoked: Story = {
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Réinitialiser'), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Continuer'));
    const body = within(context.canvasElement.ownerDocument.body);
    await userEvent.click(await within(await body.findByRole('dialog')).findByRole('button', { name: 'Réinitialiser' }));
    await canvas.findByText('Vous avez été déconnecté. Reconnectez-vous pour continuer.');
    await expect(context.canvasElement.querySelectorAll('fg-mfa-settings button, fg-mfa-settings input')).toHaveLength(0);
    await expectLayout(context);
  },
};

/** Defensive state: a live session without a factor should not exist any more. */
export const Disabled: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    await within(context.canvasElement).findByText(/La double authentification est obligatoire/);
    await expectLayout(context);
  },
};

export const EnrollPrompt: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    const field = await openPrompt(within(context.canvasElement), 'Configurer maintenant');
    await waitFor(() => expect(field).toHaveFocus());
    await expectLayout(context);
  },
};

export const EnrollScan: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    await toScan(context.canvasElement);
    await expectLayout(context);
  },
};

export const EnrollWrongCode: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    const canvas = await toScan(context.canvasElement);
    await userEvent.type(await canvas.findByLabelText('Code à 6 chiffres'), '000000');
    await userEvent.click(await buttonNamed(canvas, 'Activer'));
    await canvas.findByText('Code incorrect');
    await expectLayout(context);
  },
};

export const EnrollCodes: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    await toCodes(context.canvasElement);
    await expectLayout(context);
  },
};

export const EnrollCodesAcknowledged: Story = {
  decorators: [withMfa(DISABLED)],
  play: async (context) => {
    const canvas = await toCodes(context.canvasElement);
    await userEvent.click(await canvas.findByLabelText("J'ai enregistré mes codes de secours"));
    await waitFor(() => expect(canvas.getByRole('button', { name: 'Terminé' })).toBeEnabled());
    await expectLayout(context);
  },
};

export const PasskeyOnly: Story = {
  decorators: [withMfa(PASSKEY_ONLY)],
  play: async (context) => {
    await within(context.canvasElement).findByText('Aucune application configurée');
    await expectLayout(context);
  },
};

export const PasskeyOnlyFewCodes: Story = {
  decorators: [withMfa({ ...PASSKEY_ONLY, backupCodesRemaining: 2 })],
  play: async (context) => {
    await within(context.canvasElement).findByText(/Régénérez-en/);
    await expectLayout(context);
  },
};

export const PasskeyOnlyEnrollPrompt: Story = {
  decorators: [withMfa(PASSKEY_ONLY)],
  play: async (context) => {
    const field = await openPrompt(within(context.canvasElement), 'Ajouter une application');
    await waitFor(() => expect(field).toHaveFocus());
    await expectLayout(context);
  },
};

export const AppAndPasskey: Story = {
  decorators: [withMfa(APP_AND_PASSKEY)],
  play: async (context) => {
    await within(context.canvasElement).findByText("Supprimer l'application d'authentification");
    await expectLayout(context);
  },
};

export const AppAndPasskeyRemoveConfirmation: Story = {
  decorators: [withMfa(APP_AND_PASSKEY)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.type(await openPrompt(canvas, 'Supprimer'), PASSWORD);
    await userEvent.click(await buttonNamed(canvas, 'Continuer'));
    const dialog = await within(context.canvasElement.ownerDocument.body).findByRole('dialog');
    await expect(dialog).toHaveTextContent("Supprimer l'application d'authentification ?");
    await expectLayout(context);
  },
};
