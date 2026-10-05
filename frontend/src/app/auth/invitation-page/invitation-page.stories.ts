import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { ActivatedRoute, convertToParamMap, provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { InvitationPage } from './invitation-page';
import { AuthService } from '../auth.service';
import { provideFerrisgitAuth } from '../auth-kit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { expectPanelLayout } from '../auth-story-helpers';

const withApp = applicationConfig({
  providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()],
});
const LINK = moduleMetadata({
  providers: [
    {
      provide: ActivatedRoute,
      useValue: {
        snapshot: {
          fragment: `token=${'ab12'.repeat(16)}`,
          queryParamMap: convertToParamMap({}),
        },
      },
    },
  ],
});
const withServer = (activate$: Observable<void>) =>
  moduleMetadata({
    providers: [
      {
        provide: AuthService,
        useValue: { activate: () => activate$ } satisfies Partial<AuthService>,
      },
    ],
  });
const expectLayout = expectPanelLayout();

async function fillAndSubmit(canvasElement: HTMLElement, username = 'marie') {
  const canvas = within(canvasElement);
  await userEvent.type(await canvas.findByLabelText("Nom d'utilisateur"), username);
  await userEvent.type(canvas.getByLabelText('Nouveau mot de passe'), 'correct-horse-battery');
  await userEvent.type(canvas.getByLabelText('Confirmez le mot de passe'), 'correct-horse-battery');
  await userEvent.click(canvas.getByRole('button', { name: 'Activer mon compte' }));
  return canvas;
}

/** Where an administrator's invitation mail lands: the invitee chooses their username along with their password. */
const meta: Meta<InvitationPage> = {
  title: 'Auth/InvitationPage',
  component: InvitationPage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withApp, moduleMetadata({ providers: provideFerrisgitAuth() })],
};

export default meta;
type Story = StoryObj<InvitationPage>;

export const Default: Story = {
  decorators: [LINK, withServer(NEVER)],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
    await expectLayout(context);
  },
};

export const UsernameTaken: Story = {
  decorators: [
    LINK,
    withServer(
      throwError(
        () =>
          new HttpErrorResponse({
            status: 409,
            statusText: 'x',
            error: { error: 'username already taken' },
          }),
      ),
    ),
  ],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement, 'alice');
    await expect(await canvas.findByText("Ce nom d'utilisateur est déjà utilisé")).toBeVisible();
    await waitFor(() => expect(canvas.getByLabelText("Nom d'utilisateur")).toHaveFocus());
  },
};

export const Activated: Story = {
  decorators: [LINK, withServer(of(undefined))],
  play: async (context) => {
    const canvas = await fillAndSubmit(context.canvasElement);
    await expect(await canvas.findByRole('heading', { name: 'Votre compte est activé' })).toBeVisible();
  },
};
