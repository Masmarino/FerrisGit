import { Component, inject, provideAppInitializer } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, Observable, of, switchMap, throwError, timer } from 'rxjs';
import { AdminSettings } from './admin-settings';
import { SettingsService, SmtpSettings, SystemSettings, SystemSettingsUpdate } from '../settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';
import { inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast } from '../../shared/layout/settings-story-helpers';

@Component({ template: '' })
class Blank {}

// In-memory navigation: the nav's links are relative to the page's route, so a click moves to `/?section=<key>`.
const withRouter = applicationConfig({ providers: [provideRouter([{ path: '**', component: Blank }]), provideLocationMocks()] });
const startAt = (url: string) => applicationConfig({ providers: [provideAppInitializer(() => inject(Router).navigateByUrl(url))] });

const DOCKER_SETTINGS: SystemSettings = {
  executionEngine: 'docker-runners',
  jwtTtlHours: 8,
  maxPushSizeMb: 100,
  registrationEnabled: false,
  publicPagesEnabled: true,
  seoIndexingEnabled: false,
  k8sNamespace: null,
  k8sCacheStorageClass: null,
  runnerRegistrationTokenConfigured: true,
  logRetentionDays: 30,
  maxConcurrentJobs: 4,
  detectedK8sNamespace: null,
  detectedK8sDefaultStorageClass: null,
};

const K8S_SETTINGS: SystemSettings = {
  ...DOCKER_SETTINGS,
  executionEngine: 'kubernetes',
  jwtTtlHours: 24,
  maxPushSizeMb: 250,
  k8sNamespace: 'ferrisgit-ci',
  k8sCacheStorageClass: 'standard-rwx',
};

const K8S_AUTO_DETECTED_SETTINGS: SystemSettings = {
  ...K8S_SETTINGS,
  k8sNamespace: null,
  k8sCacheStorageClass: null,
  detectedK8sNamespace: 'ferrisgit',
  detectedK8sDefaultStorageClass: 'standard',
};

const SMTP_SETTINGS: SmtpSettings = {
  configured: true,
  host: 'smtp.mailgun.org',
  port: 587,
  security: 'starttls',
  username: 'postmaster@mg.ferrisgit.dev',
  passwordSet: true,
  fromAddress: 'noreply@ferrisgit.dev',
  fromName: 'FerrisGit',
};

function fakeSettingsService(initial: SystemSettings, options: { refuse?: boolean } = {}) {
  let current = { ...initial };
  return {
    getAdmin: () => of(current),
    getSmtp: () => of(SMTP_SETTINGS),
    updateSmtp: () => NEVER,
    testSmtp: () => NEVER,
    updateAdmin: (update: SystemSettingsUpdate): Observable<SystemSettings> =>
      timer(600).pipe(
        switchMap(() => {
          if (options.refuse) {
            return throwError(() => new Error('refused'));
          }
          current = { ...current, ...(update as Partial<SystemSettings>) };
          return of(current);
        }),
      ),
  };
}

const withSettings = (service: unknown) => moduleMetadata({ providers: [{ provide: SettingsService, useValue: service }] });

async function expectSettingsPage(canvasElement: HTMLElement, expected: string) {
  const active = await waitFor(() => {
    const link = canvasElement.querySelector<HTMLAnchorElement>('gbt-nav-tabs a[aria-current="page"]');
    if (!link || !canvasElement.querySelector('gbt-card')) {
      throw new Error('settings page not rendered yet');
    }
    return link;
  });
  await expect(canvasElement.querySelectorAll('nav'), 'navigation landmarks').toHaveLength(1);
  await expect(canvasElement.querySelectorAll('gbt-nav-tabs a[aria-current="page"]'), 'active links').toHaveLength(1);
  await expect(active.querySelector('.gbt-nav-tab__label')?.textContent, 'active section').toBe(expected);

  const doc = canvasElement.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // hidden docs frame: no layout
  }
  const layout = canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect();
  const nav = canvasElement.querySelector('.gbt-page-layout__nav')!.getBoundingClientRect();
  const main = canvasElement.querySelector('.gbt-page-layout__main')!.getBoundingClientRect();
  const h1 = canvasElement.querySelector('gbt-page-header h1')!.getBoundingClientRect();
  await expect(Math.round(h1.left), 'h1 on the nav column’s left edge').toBe(Math.round(nav.left));
  if (layout.width >= 769) {
    await expect(nav.right, 'nav column left of the section').toBeLessThanOrEqual(main.left);
    await expect(Math.round(nav.top), 'nav and section start together').toBe(Math.round(main.top));
  } else {
    await expect(nav.bottom, 'nav stacked above the section').toBeLessThanOrEqual(main.top);
  }
  await expectSettingsLayout(canvasElement);
}

const meta: Meta<AdminSettings> = {
  title: 'Settings/AdminSettings',
  component: AdminSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    withRouter,
    moduleMetadata({
      providers: [
        { provide: SettingsService, useValue: fakeSettingsService(DOCKER_SETTINGS) },
        { provide: PageTitleService, useValue: { set: () => {} } },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inShellContentArea,
  ],
};

export default meta;
type Story = StoryObj<AdminSettings>;

export const DockerRunners: Story = {
  decorators: [startAt('/')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Exécution'),
};

export const Kubernetes: Story = {
  decorators: [startAt('/'), withSettings(fakeSettingsService(K8S_SETTINGS))],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Exécution'),
};

export const KubernetesAutoDetected: Story = {
  decorators: [startAt('/'), withSettings(fakeSettingsService(K8S_AUTO_DETECTED_SETTINGS))],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Exécution'),
};

export const Security: Story = {
  decorators: [startAt('/?section=security')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Sécurité'),
};

export const Registration: Story = {
  decorators: [startAt('/?section=security'), withSettings(fakeSettingsService({ ...DOCKER_SETTINGS, registrationEnabled: true }))],
  play: async ({ canvasElement }) => {
    await expectSettingsPage(canvasElement, 'Sécurité');
    const toggle = await waitFor(() => {
      const input = canvasElement.querySelector<HTMLInputElement>('[data-field="registrationEnabled"] input[role="switch"]');
      if (!input) throw new Error('registration switch not rendered yet');
      return input;
    });
    await waitFor(() => expect(toggle.checked, 'registration switch on').toBe(true));
  },
};

export const PublicPages: Story = {
  decorators: [startAt('/?section=security'), withSettings(fakeSettingsService({ ...DOCKER_SETTINGS, publicPagesEnabled: true, seoIndexingEnabled: true }))],
  play: async ({ canvasElement }) => {
    await expectSettingsPage(canvasElement, 'Sécurité');
    const seo = await waitFor(() => {
      const input = canvasElement.querySelector<HTMLInputElement>('[data-field="seoIndexingEnabled"] input[role="switch"]');
      if (!input) throw new Error('indexing switch not rendered yet');
      return input;
    });
    await waitFor(() => expect(seo.checked, 'indexing switch on').toBe(true));
  },
};

export const PublicPagesOff: Story = {
  decorators: [startAt('/?section=security'), withSettings(fakeSettingsService({ ...DOCKER_SETTINGS, publicPagesEnabled: false }))],
  play: async ({ canvasElement }) => {
    await expectSettingsPage(canvasElement, 'Sécurité');
    await waitFor(() => expect(canvasElement.querySelector<HTMLInputElement>('[data-field="seoIndexingEnabled"] input[role="switch"]')?.disabled, 'indexing switch greyed out').toBe(true));
  },
};

export const Email: Story = {
  decorators: [startAt('/?section=email')],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'E-mail'),
};

export const SavesRefused: Story = {
  decorators: [startAt('/'), withSettings(fakeSettingsService(K8S_SETTINGS, { refuse: true }))],
  play: ({ canvasElement }) => expectSettingsPage(canvasElement, 'Exécution'),
};

export const Loading: Story = {
  decorators: [startAt('/'), withSettings({ getAdmin: () => NEVER, updateAdmin: () => NEVER })],
};

export const LoadFailed: Story = {
  decorators: [startAt('/'), withSettings({ getAdmin: () => throwError(() => new Error('boom')), updateAdmin: () => NEVER })],
};
