import { Component, inject, provideAppInitializer } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { DevicePage } from './device-page';
import { DeviceLoginService, DeviceRequest } from './device-login.service';
import { PageTitleService } from '../../shell/page-title.service';
import { inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, rendered } from '../../shared/layout/settings-story-helpers';
import { REQUEST_OPTIONS, dismissedPrompt, fakeAssertion, stubPasskeyBrowser } from '../../shared/webauthn-testing';

@Component({ template: '' })
class Blank {}

const withRouter = applicationConfig({ providers: [provideRouter([{ path: '**', component: Blank }]), provideLocationMocks()] });
const startAt = (url: string) => applicationConfig({ providers: [provideAppInitializer(() => inject(Router).navigateByUrl(url))] });

const REQUEST: DeviceRequest = {
  userCode: 'BCDF-GHJK',
  clientName: 'fgit sur portable-de-florian',
  scopes: ['repositories:write', 'issues:write', 'merge-requests:write', 'pipelines:read', 'releases:read'],
  tokenLifetimeDays: 90,
  requestedAt: '2026-11-02T10:00:00Z',
  expiresAt: '2026-11-02T10:15:00Z',
  hasTotp: true,
  hasPasskey: true,
};

function fakeDeviceService(overrides: Partial<Record<keyof DeviceLoginService, unknown>> = {}) {
  return {
    find: () => of(REQUEST),
    startPasskey: () => of({ challengeId: 'c1', publicKey: REQUEST_OPTIONS }),
    approve: () => of(undefined),
    deny: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Partial<Record<keyof DeviceLoginService, unknown>>) => moduleMetadata({ providers: [{ provide: DeviceLoginService, useValue: fakeDeviceService(overrides) }] });

const refused = (status: number, error: string) => () => throwError(() => ({ status, error: { error } }));

const meta: Meta<DevicePage> = {
  title: 'Account/DevicePage',
  component: DevicePage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withRouter,
    startAt('/?code=BCDF-GHJK'),
    moduleMetadata({
      providers: [
        { provide: DeviceLoginService, useValue: fakeDeviceService() },
        { provide: PageTitleService, useValue: { set: () => {} } },
      ],
    }),
    inShellContentArea,
  ],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSettingsLayout(canvasElement);
  },
};

export default meta;
type Story = StoryObj<DevicePage>;

/** Stubs WebAuthn for the story and restores it afterwards. */
const withPasskeys = (get: () => Promise<unknown>) => () => {
  const restore = stubPasskeyBrowser({ get, create: () => Promise.reject(dismissedPrompt()) });
  return () => restore();
};

export const TotpAndPasskey: Story = {
  beforeEach: withPasskeys(() => Promise.resolve(fakeAssertion())),
};

export const TotpOnly: Story = {
  decorators: [withService({ find: () => of({ ...REQUEST, hasPasskey: false }) })],
};

export const PasskeyOnly: Story = {
  beforeEach: withPasskeys(() => Promise.resolve(fakeAssertion())),
  decorators: [withService({ find: () => of({ ...REQUEST, hasTotp: false }) })],
};

export const EnterCode: Story = {
  decorators: [startAt('/')],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-input');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Code affiché par le terminal'), 'trop-court{Enter}');
    await waitFor(() => expect(canvasElement.querySelector('gbt-input')?.textContent).toContain('8 lettres ou chiffres'));
    await expectSettingsLayout(canvasElement);
  },
};

export const Loading: Story = {
  decorators: [withService({ find: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
  },
};

export const LongClientName: Story = {
  decorators: [withService({ find: () => of({ ...REQUEST, clientName: `<b>${'x'.repeat(90)}</b>` }) })],
  play: async ({ canvasElement }) => {
    const client = await rendered(canvasElement, '.device-page__client');
    await expect(client.children.length).toBe(0);
    await expectSettingsLayout(canvasElement);
  },
};

export const NotFound: Story = {
  decorators: [withService({ find: refused(404, 'device request') })],
};

export const RateLimited: Story = {
  decorators: [withService({ find: refused(429, 'too many attempts, try again later') })],
};

export const LoadFailed: Story = {
  decorators: [withService({ find: refused(500, 'internal server error') })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-alert');
  },
};

export const WrongCode: Story = {
  decorators: [withService({ find: () => of({ ...REQUEST, hasPasskey: false }), approve: refused(400, 'invalid code') })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'form');
    const canvas = within(canvasElement);
    await userEvent.type(canvas.getByLabelText('Code à 6 chiffres'), '000000{Enter}');
    await rendered(canvasElement, '.device-page__error');
    await expect(canvasElement.querySelector('.device-page__error')?.textContent).toContain('Code incorrect');
    await expect(canvasElement.querySelector('.device-page__code')?.textContent).toBe('BCDF-GHJK');
    await expectSettingsLayout(canvasElement);
  },
};

export const Approved: Story = {
  decorators: [withService({ find: () => of({ ...REQUEST, hasPasskey: false }) })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'form');
    await userEvent.type(within(canvasElement).getByLabelText('Code à 6 chiffres'), '123456{Enter}');
    const result = await rendered(canvasElement, '.device-page__result');
    await expect(result.textContent).toContain('Vous pouvez retourner à votre terminal.');
    await expectSettingsLayout(canvasElement);
  },
};

export const Denied: Story = {
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'form');
    await userEvent.click(within(canvasElement).getByRole('button', { name: 'Refuser' }));
    const result = await rendered(canvasElement, '.device-page__result');
    await expect(result.textContent).toContain("Le terminal n'a reçu aucun jeton.");
  },
};
