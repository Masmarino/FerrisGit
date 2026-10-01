import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { GroupDetail } from './group-detail';
import { Group, GroupMember, GroupsService } from '../groups.service';
import { Repository, RepositoriesService } from '../../repositories/repositories.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, inShellContentArea } from '../../shared/layout/page-story-helpers';
import { MANY_REPOSITORIES, expectWorkspaceLayout, repository } from '../../repositories/workspace-grid/workspace-story-helpers';

// `provideRouter` returns EnvironmentProviders, so it goes in `applicationConfig`, not `moduleMetadata`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const CHILDREN: Group[] = [
  { id: 'child-1', parentGroupId: 'group-1', name: 'backend', description: 'Services et API de la plateforme', createdAt: daysAgo(40) },
  { id: 'child-2', parentGroupId: 'group-1', name: 'frontend', description: 'Applications web et design system', createdAt: daysAgo(12) },
];

const REPOS: Repository[] = [
  repository({ path: ['acme', 'facturation'], description: 'Facturation et devis', role: 'maintainer', createdAt: daysAgo(2) }),
  repository({ path: ['acme', 'api-publique'], description: '', role: 'maintainer', visibility: 'public', createdAt: daysAgo(30) }),
  repository({ path: ['acme', 'documentation'], description: 'Guides internes et procédures', role: 'maintainer', visibility: 'public', createdAt: daysAgo(90) }),
];

const MEMBERS: GroupMember[] = [
  { userId: 'u1', username: 'camille.martin', role: 'maintainer', createdAt: daysAgo(200) },
  { userId: 'u2', username: 'julien.dubois', role: 'contributor', createdAt: daysAgo(120) },
  { userId: 'u3', username: 'lea.bernard', role: 'contributor', createdAt: daysAgo(80) },
  { userId: 'u4', username: 'nora.haddad', role: 'reader', createdAt: daysAgo(30) },
  { userId: 'u5', username: 'paul.lefevre', role: 'reader', createdAt: daysAgo(9) },
  { userId: 'u6', username: 'ines.moreau', role: 'reader', createdAt: daysAgo(2) },
];

function withData(options: {
  children?: Group[];
  repos?: Repository[];
  members?: GroupMember[];
  listChildren?: () => Observable<Group[]>;
  listForGroup?: () => Observable<Repository[]>;
  listMembers?: () => Observable<GroupMember[]>;
} = {}) {
  return moduleMetadata({
    providers: [
      {
        provide: GroupsService,
        useValue: {
          listChildren: options.listChildren ?? (() => of(options.children ?? CHILDREN)),
          listMembers: options.listMembers ?? (() => of(options.members ?? MEMBERS)),
          listWritable: () => of([{ id: 'group-1', path: 'acme' }]),
          createSubgroup: () => of({} as Group),
          delete: () => of(undefined),
        },
      },
      {
        provide: RepositoriesService,
        useValue: {
          listForGroup: options.listForGroup ?? (() => of(options.repos ?? REPOS)),
          create: () => of(REPOS[0]),
          delete: () => of(undefined),
          cloneUrl: (path: string[]) => `https://ferrisgit.example/${path.join('/')}.git`,
        },
      },
      { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
    ],
  });
}

const meta: Meta<GroupDetail> = {
  title: 'Groups/GroupDetail',
  component: GroupDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  argTypes: {
    groupId: { control: 'text' },
    role: { control: 'text' },
  },
  decorators: [withApp, inShellContentArea],
};

export default meta;
type Story = StoryObj<GroupDetail>;

export const Maintainer: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme'] },
  decorators: [withData()],
  play: expectWorkspaceLayout,
};

export const Reader: Story = {
  args: { groupId: 'group-1', role: 'reader', path: ['acme'] },
  decorators: [withData({ repos: REPOS.map((r) => ({ ...r, role: 'reader' as const })) })],
  play: async (context) => {
    await expectWorkspaceLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-button--primary')).toBeNull();
  },
};

export const LongNames: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme-france', 'produits', 'applications-mobiles', 'equipe-paiements-et-facturation-internationale'] },
  decorators: [
    withData({
      children: [{ id: 'c', parentGroupId: 'group-1', name: 'sdk-paiement-sans-contact-ios-et-android-nouvelle-generation', description: 'Une description très longue qui dépasse largement la largeur de la ligne du sous-groupe.', createdAt: daysAgo(3) }],
      repos: [repository({ path: ['acme-france', 'produits', 'applications-mobiles', 'equipe-paiements-et-facturation-internationale', 'passerelle-de-paiement-carte-bancaire'], role: 'maintainer', createdAt: daysAgo(1) })],
      members: [{ userId: 'u', username: 'maximilien.de-la-tour-d-auvergne-et-de-bouillon', role: 'maintainer', createdAt: daysAgo(10) }],
    }),
  ],
  play: expectWorkspaceLayout,
};

export const ManyRows: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['camille.martin'] },
  decorators: [withData({ children: CHILDREN, repos: MANY_REPOSITORIES })],
  play: expectWorkspaceLayout,
};

export const Empty: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme', 'empty-group'] },
  decorators: [withData({ children: [], repos: [], members: MEMBERS.slice(0, 1) })],
};

export const Loading: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme'] },
  decorators: [withData({ listChildren: () => NEVER, listForGroup: () => NEVER, listMembers: () => NEVER })],
};

export const LoadError: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme'] },
  decorators: [withData({ listChildren: () => throwError(() => new Error('500')), listMembers: () => throwError(() => new Error('500')) })],
};

export const CreateSubgroupDialog: Story = {
  args: { groupId: 'group-1', role: 'maintainer', path: ['acme'] },
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const button = await waitFor(() => {
      const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouveau sous-groupe'));
      if (!found) throw new Error('"Nouveau sous-groupe" not rendered yet');
      return found;
    });
    button.click();
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('[role="dialog"]')).not.toBeNull());
  },
};
