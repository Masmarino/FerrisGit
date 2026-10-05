import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, fireEvent, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, switchMap, throwError, timer } from 'rxjs';
import { RepositoryGeneralSettings } from './repository-general-settings';
import { Repository, RepositoriesService } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { atPhoneWidth, inDarkTheme, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const PRIVATE_REPOSITORY = repositoryFixture({ description: 'Une plateforme Git auto-hébergée, écrite en Rust.', path: ['florian', 'ferrisgit'] });

function fakeRepositoriesService(overrides: Record<string, unknown> = {}, initial: Repository = PRIVATE_REPOSITORY) {
  let current = { ...initial };
  return {
    getById: () => of(current),
    update: (_id: string, update: Partial<Repository>) => {
      current = { ...current, ...update };
      return of(current);
    },
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>, initial?: Repository) =>
  moduleMetadata({ providers: [{ provide: RepositoriesService, useValue: fakeRepositoriesService(overrides, initial) }] });

const meta: Meta<RepositoryGeneralSettings> = {
  title: 'Repositories/Settings/RepositoryGeneralSettings',
  component: RepositoryGeneralSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: RepositoriesService, useValue: fakeRepositoriesService() },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inSettingsColumn,
  ],
  args: { repositoryId: 'repo-1' },
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSettingsLayout(canvasElement);
  },
};

export default meta;
type Story = StoryObj<RepositoryGeneralSettings>;

export const Private: Story = {};

export const Public: Story = {
  decorators: [withService({}, { ...PRIVATE_REPOSITORY, visibility: 'public' })],
};

export const Dark: Story = { decorators: [inDarkTheme] };

export const AtPhoneWidth: Story = { decorators: [atPhoneWidth] };

export const ConfirmMakePublic: Story = {
  play: async ({ canvasElement }) => {
    const options = await waitFor(() => {
      const found = canvasElement.querySelectorAll<HTMLElement>('[data-field="visibility"] [role="radio"]');
      if (found.length === 0) throw new Error('not rendered yet');
      return found;
    });
    await userEvent.click(options[1]);
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector('gbt-confirm-danger-modal');
      if (!found) throw new Error('dialog not open yet');
      return found;
    });
    await expect(dialog.textContent).toContain('Rendre ce dépôt public');
  },
};

export const ConfirmMakePrivateInDark: Story = {
  decorators: [withService({}, { ...PRIVATE_REPOSITORY, visibility: 'public' }), inDarkTheme],
  play: async ({ canvasElement }) => {
    const options = await waitFor(() => {
      const found = canvasElement.querySelectorAll<HTMLElement>('[data-field="visibility"] [role="radio"]');
      if (found.length === 0) throw new Error('not rendered yet');
      return found;
    });
    await userEvent.click(options[0]);
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector('gbt-confirm-danger-modal');
      if (!found) throw new Error('dialog not open yet');
      return found;
    });
    await expect(dialog.textContent).toContain('disparaîtra du catalogue public');
  },
};

export const DescriptionSaved: Story = {
  play: async ({ canvasElement }) => {
    const description = await rendered<HTMLTextAreaElement>(canvasElement, '[data-field="description"] textarea');
    await userEvent.clear(description);
    await userEvent.type(description, 'Un nouveau texte.');
    fireEvent.blur(description); // blur() does nothing while the preview frame lacks focus.
    await waitFor(() => expect(canvasElement.querySelector('[data-field="description"] [role="status"]')?.textContent?.trim()).toBe('Enregistré'));
    await expectSettingsLayout(canvasElement);
  },
};

export const SaveFailed: Story = {
  // Fail a bit later, like a real HTTP error, not synchronously.
  decorators: [withService({ update: () => timer(200).pipe(switchMap(() => throwError(() => ({ status: 500 })))) })],
  play: async ({ canvasElement }) => {
    const description = await rendered<HTMLTextAreaElement>(canvasElement, '[data-field="description"] textarea');
    await userEvent.clear(description);
    await userEvent.type(description, 'Un nouveau texte.');
    fireEvent.blur(description);
    await waitFor(() => expect(canvasElement.querySelector('[data-field="description"] [role="status"]')?.textContent?.trim()).toBe('Non enregistré'));
  },
};

export const Loading: Story = {
  decorators: [withService({ getById: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[aria-busy="true"]');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ getById: () => throwError(() => ({ status: 500 })) })],
};
