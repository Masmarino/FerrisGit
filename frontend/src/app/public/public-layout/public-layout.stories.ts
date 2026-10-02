import { Component, inject, provideAppInitializer, signal } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { Card } from '@masmarino/gabarit';
import { PublicLayout } from './public-layout';
import { AuthService } from '../../auth/auth.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PublicRepositoryContextService } from '../public-repository-context.service';
import { atPhoneWidth, inDarkTheme, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';

@Component({
  standalone: true,
  imports: [Card],
  template: `
    <h1 style="margin: 0 0 1rem; font-size: 1.5rem;">Contenu de la page</h1>
    <gbt-card variant="outlined"><p style="margin: 0;">Le contenu de chaque page publique s'affiche dans cette colonne centrée.</p></gbt-card>
  `,
})
class SamplePage {}

const withRouter = applicationConfig({
  providers: [provideRouter([{ path: '**', component: SamplePage }]), provideLocationMocks(), provideAppInitializer(() => inject(Router).navigateByUrl('/repositories/alice/hello'))],
});

const signedIn = (value: boolean) =>
  applicationConfig({
    providers: [
      { provide: AuthService, useValue: { isAuthenticated: signal(value).asReadonly() } satisfies Pick<AuthService, 'isAuthenticated'> },
      { provide: RepositoryContextService, useClass: PublicRepositoryContextService },
    ],
  });

async function expectHeader({ canvasElement }: { canvasElement: HTMLElement }) {
  const header = await waitFor(() => {
    const found = canvasElement.querySelector('.public-layout__header');
    if (!found) throw new Error('layout not rendered yet');
    return found;
  });
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
  const box = header.getBoundingClientRect();
  for (const part of Array.from(header.querySelectorAll('.public-layout__home, .public-layout__search, .public-layout__docs, .public-layout__account a'))) {
    const rect = part.getBoundingClientRect();
    await expect(rect.left >= box.left - 0.5 && rect.right <= box.right + 0.5, `${part.className} stays inside the header`).toBe(true);
  }
}

const meta: Meta<PublicLayout> = {
  title: 'Public/PublicLayout',
  component: PublicLayout,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, withRouter],
};

export default meta;
type Story = StoryObj<PublicLayout>;

export const Anonymous: Story = {
  decorators: [signedIn(false)],
  play: expectHeader,
};

export const SignedIn: Story = {
  decorators: [signedIn(true)],
  play: async (context) => {
    await expectHeader(context);
    await expect(context.canvasElement.querySelector('.public-layout__account a')?.textContent?.trim()).toBe('Mes dépôts');
  },
};

export const Dark: Story = {
  decorators: [signedIn(false), inDarkTheme],
  play: expectHeader,
};

export const Phone: Story = {
  decorators: [signedIn(false), atPhoneWidth],
  play: async (context) => {
    await expectHeader(context);
    const search = context.canvasElement.querySelector('.public-layout__search')!.getBoundingClientRect();
    const home = context.canvasElement.querySelector('.public-layout__home')!.getBoundingClientRect();
    await expect(search.top, 'the search box moves under the logo').toBeGreaterThanOrEqual(home.bottom - 0.5);
  },
};

export const PhoneDark: Story = {
  decorators: [signedIn(false), atPhoneWidth, inDarkTheme],
  play: expectHeader,
};
