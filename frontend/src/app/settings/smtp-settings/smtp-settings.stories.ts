import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, Observable, of, throwError, timer, switchMap } from 'rxjs';
import { SmtpSettings } from './smtp-settings';
import { SettingsService, SmtpSettings as SmtpSettingsData, SmtpTestResult } from '../settings.service';
import { GbtToastService } from '@masmarino/gabarit';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const CONFIGURED: SmtpSettingsData = {
  configured: true,
  host: 'smtp.mailgun.org',
  port: 587,
  security: 'starttls',
  username: 'postmaster@mg.ferrisgit.dev',
  passwordSet: true,
  fromAddress: 'noreply@ferrisgit.dev',
  fromName: 'FerrisGit',
};

const NOT_CONFIGURED: SmtpSettingsData = {
  configured: false,
  host: '',
  port: 587,
  security: 'starttls',
  username: '',
  passwordSet: false,
  fromAddress: '',
  fromName: 'FerrisGit',
};

const NO_ENCRYPTION: SmtpSettingsData = {
  ...CONFIGURED,
  host: 'mail.interne.lan',
  port: 25,
  security: 'none',
  username: '',
  passwordSet: false,
};

function fakeSettingsService(settings: SmtpSettingsData | 'loading' | 'failed', test: () => Observable<SmtpTestResult> = () => of({ sent: true })) {
  return {
    getSmtp: () => (settings === 'loading' ? NEVER : settings === 'failed' ? throwError(() => new Error('boom')) : of(settings)),
    updateSmtp: () => NEVER,
    testSmtp: () => timer(300).pipe(switchMap(test)),
  };
}

const withSettings = (service: unknown) => moduleMetadata({ providers: [{ provide: SettingsService, useValue: service }] });

async function expectSmtpLayout(canvasElement: HTMLElement) {
  await expectSettingsLayout(canvasElement);
  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return;
  }
  const box = (selector: string) => canvasElement.querySelector(selector)!.getBoundingClientRect();
  const host = box('[data-field="host"] .gbt-input__wrapper');
  const port = box('[data-field="port"] .gbt-input__wrapper');
  await expect(Math.round(host.height), 'host and port inputs one height').toBe(Math.round(port.height));
  await expect(Math.round(host.top), 'host and port inputs on one line').toBe(Math.round(port.top));
  for (const card of Array.from(canvasElement.querySelectorAll('gbt-card .smtp-settings__actions'))) {
    // The card's body wrapper is `display: contents`: the box is `.gbt-card`, whose content edge is inside its border and padding.
    const inner = card.closest('.gbt-card')!;
    const style = getComputedStyle(inner);
    const edge = inner.getBoundingClientRect().right - parseFloat(style.borderRightWidth) - parseFloat(style.paddingRight);
    const button = card.querySelector('.gbt-button')!.getBoundingClientRect();
    await expect(Math.round(button.right), 'button on the card content edge').toBe(Math.round(edge));
  }
}

const meta: Meta<SmtpSettings> = {
  title: 'Settings/SmtpSettings',
  component: SmtpSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: SettingsService, useValue: fakeSettingsService(CONFIGURED) },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inSettingsColumn,
  ],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSmtpLayout(canvasElement);
  },
};

export default meta;
type Story = StoryObj<SmtpSettings>;

export const Configured: Story = {};

export const NotConfigured: Story = {
  decorators: [withSettings(fakeSettingsService(NOT_CONFIGURED))],
};

export const NoEncryption: Story = {
  decorators: [withSettings(fakeSettingsService(NO_ENCRYPTION))],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[data-warning="no-encryption"]');
    await expectSmtpLayout(canvasElement);
  },
};

export const Loading: Story = {
  decorators: [withSettings(fakeSettingsService('loading'))],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[aria-busy="true"]');
  },
};

export const LoadFailed: Story = {
  decorators: [withSettings(fakeSettingsService('failed'))],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-alert .gbt-alert[data-variant="error"]');
    await expectSettingsLayout(canvasElement);
  },
};

async function sendTest(canvasElement: HTMLElement) {
  const recipient = await rendered<HTMLInputElement>(canvasElement, '[data-field="testRecipient"] input');
  await userEvent.type(recipient, 'admin@ferrisgit.dev');
  const send = await rendered<HTMLButtonElement>(canvasElement, '.smtp-settings__actions:has(.smtp-settings__test-status) button[type="submit"]');
  await waitFor(() => expect(send.disabled).toBe(false));
  await userEvent.click(send);
}

export const TestSucceeded: Story = {
  play: async ({ canvasElement }) => {
    await sendTest(canvasElement);
    await rendered(canvasElement, '.smtp-settings__banner[data-state="sent"]');
    await expectSmtpLayout(canvasElement);
  },
};

export const TestFailed: Story = {
  decorators: [
    withSettings(
      fakeSettingsService(CONFIGURED, () => of({ sent: false, error: 'authentification refusée par smtp.mailgun.org : 535 5.7.8 Username and Password not accepted' })),
    ),
  ],
  play: async ({ canvasElement }) => {
    await sendTest(canvasElement);
    await rendered(canvasElement, '.smtp-settings__banner[data-state="failed"]');
    await expectSmtpLayout(canvasElement);
  },
};

export const ValidationErrors: Story = {
  decorators: [withSettings(fakeSettingsService(NOT_CONFIGURED))],
  play: async ({ canvasElement }) => {
    const port = await rendered<HTMLInputElement>(canvasElement, '[data-field="port"] input');
    await userEvent.clear(port);
    await userEvent.type(port, '70000');
    await userEvent.type(await rendered(canvasElement, '[data-field="username"] input'), 'mailer');
    await userEvent.click(await rendered(canvasElement, '.smtp-settings__actions button[type="submit"]'));
    await waitFor(() => expect(canvasElement.querySelectorAll('.gbt-input__error').length).toBeGreaterThanOrEqual(3));
    await expectSmtpLayout(canvasElement);
  },
};
