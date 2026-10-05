import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { of } from 'rxjs';
import { WorkspaceGrid, WorkspaceGroupItem } from './workspace-grid';
import { RepositoriesService } from '../repositories.service';
import { GroupsService } from '../../groups/groups.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import { withRouterAndIcons } from '../repository-story-fixtures';
import { LONG_REPOSITORIES, MANY_REPOSITORIES, REPOSITORIES, expectWorkspaceLayout } from './workspace-story-helpers';

const withServices = moduleMetadata({
  providers: [
    {
      provide: RepositoriesService,
      useValue: {
        delete: () => of(undefined),
        cloneUrl: (path: string[]) => `https://ferrisgit.example/${path.join('/')}.git`,
      },
    },
    { provide: GroupsService, useValue: { delete: () => of(undefined) } },
    { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
  ],
});

const GROUPS: WorkspaceGroupItem[] = [
  { id: 'group-1', name: 'acme-france', link: ['/repositories', 'acme-france'], path: 'acme-france', role: 'maintainer' },
  { id: 'group-2', name: 'web', link: ['/repositories', 'acme-france', 'produits', 'web'], path: 'acme-france/produits/web', role: 'contributor' },
  { id: 'group-3', name: 'communaute', link: ['/repositories', 'communaute'], path: 'communaute', role: 'reader' },
];

const meta: Meta<WorkspaceGrid> = {
  title: 'Repositories/WorkspaceGrid',
  component: WorkspaceGrid,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: {
    searchLabel: 'Rechercher un dépôt ou un groupe',
    emptyHeading: 'Aucun dépôt pour le moment',
    emptyMessage: 'Créez votre premier dépôt pour commencer.',
    groups: [],
    repositories: [],
    loading: false,
    failed: false,
    heading: null,
  },
  decorators: [withRouterAndIcons, withServices, inShellContentArea],
};

export default meta;
type Story = StoryObj<WorkspaceGrid>;

export const Populated: Story = {
  args: { groups: GROUPS, repositories: REPOSITORIES },
  play: expectWorkspaceLayout,
};

export const RowMenuOpen: Story = {
  args: { groups: GROUPS, repositories: REPOSITORIES },
  play: async (context) => {
    await expectWorkspaceLayout(context);
    const canvas = context.canvasElement;
    const trigger = canvas.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!;
    trigger.click();
    await waitFor(() => expect(canvas.querySelectorAll('[role="menuitem"]').length).toBe(2));
    const [copy, remove] = Array.from(canvas.querySelectorAll<HTMLElement>('[role="menuitem"]'));
    await expect(copy.textContent?.trim()).toBe('Copier le chemin');
    await expect(remove.textContent?.trim()).toBe('Supprimer');
    await expect(remove.getAttribute('data-variant')).toBe('danger');
    await expect(getComputedStyle(remove).color, 'the danger item is not the default colour').not.toBe(getComputedStyle(copy).color);
    await waitFor(() => expect(canvas.ownerDocument.activeElement).toBe(copy));
  },
};

export const DeleteCancelledReturnsFocus: Story = {
  args: { groups: GROUPS, repositories: REPOSITORIES },
  play: async (context) => {
    await expectWorkspaceLayout(context);
    const canvas = context.canvasElement;
    const trigger = canvas.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!;
    trigger.click();
    await waitFor(() => expect(canvas.querySelectorAll('[role="menuitem"]').length).toBe(2));
    Array.from(canvas.querySelectorAll<HTMLElement>('[role="menuitem"]'))
      .find((item) => item.textContent?.trim() === 'Supprimer')!
      .click();
    await waitFor(() => expect(canvas.querySelector('gbt-confirm-danger-modal [role="dialog"], gbt-confirm-danger-modal dialog')).toBeTruthy());
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(canvas.ownerDocument.activeElement).toBe(trigger));
  },
};

export const WithHeading: Story = {
  args: { heading: 'Sous-groupes et dépôts', groups: GROUPS.slice(0, 1), repositories: REPOSITORIES.slice(0, 3) },
  play: expectWorkspaceLayout,
};

export const RepositoriesOnly: Story = {
  args: { repositories: REPOSITORIES.slice(0, 2) },
  play: expectWorkspaceLayout,
};

export const LongNames: Story = {
  args: { groups: [{ id: 'g', name: 'equipe-paiements', link: ['/repositories', 'x'], path: 'acme-france/produits/applications-mobiles/equipe-paiements-et-facturation-internationale', role: 'maintainer' }], repositories: LONG_REPOSITORIES },
  play: expectWorkspaceLayout,
};

export const ManyRows: Story = {
  args: { repositories: MANY_REPOSITORIES },
  play: expectWorkspaceLayout,
};

export const Empty: Story = {};

export const Loading: Story = {
  args: { loading: true },
};

export const LoadError: Story = {
  args: { failed: true },
};
