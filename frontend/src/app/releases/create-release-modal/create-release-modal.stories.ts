import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { CreateReleaseModal } from './create-release-modal';
import { ReleasesService, TagSummary } from '../releases.service';
import { BranchInfo, MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';

// provideFerrisgitIcons returns EnvironmentProviders, which only fits in an ApplicationConfig.
const withIcons = applicationConfig({ providers: [provideFerrisgitIcons()] });

const BRANCHES: BranchInfo[] = [
  { name: 'main', tipSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678', isDefault: true },
  { name: 'develop', tipSha: 'b2c3d4e5f60718293a4b5c6d7e8f901234567890', isDefault: false },
  { name: 'release/2.4', tipSha: 'c3d4e5f60718293a4b5c6d7e8f90123456789012', isDefault: false },
];

const TAGS: TagSummary[] = [
  { name: 'v2.3.1', targetSha: 'd4e5f60718293a4b5c6d7e8f9012345678901234' },
  { name: 'v2.3.0', targetSha: 'e5f60718293a4b5c6d7e8f901234567890123456' },
];

function withRefs(tags: TagSummary[], create: () => unknown = () => of(undefined)) {
  return moduleMetadata({
    providers: [
      { provide: ReleasesService, useValue: { listTags: () => of(tags), create, deleteTag: () => of(undefined) } },
      { provide: MergeRequestsService, useValue: { listBranches: () => of(BRANCHES) } },
    ],
  });
}

const meta: Meta<CreateReleaseModal> = {
  title: 'Releases/CreateReleaseModal',
  component: CreateReleaseModal,
  tags: ['autodocs'],
  args: { repositoryId: 'repo-1' },
  decorators: [withIcons],
};

export default meta;
type Story = StoryObj<CreateReleaseModal>;

export const NewTag: Story = {
  decorators: [withRefs(TAGS)],
};

export const ExistingTag: Story = {
  decorators: [withRefs(TAGS)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('combobox', { name: /Tag/ }));
    await userEvent.click(await body.findByRole('option', { name: 'v2.3.1' }));
  },
};

export const TagConflict: Story = {
  decorators: [withRefs(TAGS, () => throwError(() => ({ status: 400 })))],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.type(await body.findByLabelText(/Nom du nouveau tag/), 'v2.3.1');
    await userEvent.type(await body.findByLabelText(/Titre/), 'Version 2.3.1 — correctifs');
    await userEvent.click(await body.findByRole('button', { name: 'Créer la release' }));
    await body.findByRole('button', { name: 'Supprimer le tag « v2.3.1 » et réessayer' });
  },
};

export const MissingFields: Story = {
  decorators: [withRefs(TAGS)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('button', { name: 'Créer la release' }));
    await waitFor(() => expect(body.getByRole('alert').textContent).toContain('sont requis'));
  },
};

export const Creating: Story = {
  decorators: [withRefs(TAGS, () => NEVER)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.type(await body.findByLabelText(/Nom du nouveau tag/), 'v2.4.0');
    await userEvent.type(await body.findByLabelText(/Titre/), 'Version 2.4');
    await userEvent.click(await body.findByRole('button', { name: 'Créer la release' }));
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('button[type="submit"]')?.getAttribute('aria-busy')).toBe('true'));
  },
};
