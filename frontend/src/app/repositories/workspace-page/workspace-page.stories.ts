import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { ActivatedRoute, convertToParamMap } from '@angular/router';
import { WorkspacePage } from './workspace-page';
import { Repository, RepositoriesService } from '../repositories.service';
import { GroupMembership, GroupsService, WritableGroup } from '../../groups/groups.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import { withRouterAndIcons } from '../repository-story-fixtures';
import { LONG_MEMBERSHIPS, LONG_REPOSITORIES, MANY_REPOSITORIES, MEMBERSHIPS, REPOSITORIES, expectWorkspaceLayout } from '../workspace-grid/workspace-story-helpers';

const WRITABLE: WritableGroup[] = [
  { id: 'group-1', path: 'acme-france' },
  { id: 'group-2', path: 'acme-france/produits' },
];

function withData(options: {
  repos?: Repository[];
  starred?: Repository[];
  groups?: GroupMembership[];
  tab?: string;
  list?: () => Observable<Repository[]>;
  listMember?: () => Observable<GroupMembership[]>;
} = {}) {
  const repos = options.repos ?? REPOSITORIES;
  const starred = options.starred ?? repos.filter((r) => (r.starCount ?? 0) > 10);
  const providers: unknown[] = [
    {
      provide: RepositoriesService,
      useValue: {
        list: options.list ?? ((filter: { starred?: boolean } = {}) => of(filter.starred ? starred : repos)),
        create: () => of(repos[0]),
        delete: () => of(undefined),
        cloneUrl: (path: string[]) => `https://ferrisgit.example/${path.join('/')}.git`,
      },
    },
    {
      provide: GroupsService,
      useValue: {
        listMember: options.listMember ?? (() => of(options.groups ?? MEMBERSHIPS)),
        listWritable: () => of(WRITABLE),
        createRoot: () => of({ id: 'group-9', parentGroupId: null, name: 'nouveau', description: '', createdAt: '2026-01-01T00:00:00Z' }),
        delete: () => of(undefined),
      },
    },
    { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
  ];
  if (options.tab) {
    providers.push({ provide: ActivatedRoute, useValue: { queryParamMap: of(convertToParamMap({ tab: options.tab })) } });
  }
  return moduleMetadata({ providers: providers as never });
}

const meta: Meta<WorkspacePage> = {
  title: 'Repositories/WorkspacePage',
  component: WorkspacePage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<WorkspacePage>;

export const AllTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectWorkspaceLayout(context);
    const tabs = Array.from(context.canvasElement.querySelectorAll('.workspace-page__tabs [role="radio"]'), (b) => b.textContent?.trim());
    await expect(tabs).toEqual(['Tous (9)', 'Mes dépôts (2)', 'Favoris', 'Groupes (4)']);
  },
};

export const StarredTab: Story = {
  decorators: [withData({ tab: 'starred' })],
  play: expectWorkspaceLayout,
};

export const GroupsTab: Story = {
  decorators: [withData({ tab: 'groups' })],
  play: expectWorkspaceLayout,
};

export const LongNames: Story = {
  decorators: [withData({ repos: LONG_REPOSITORIES, groups: LONG_MEMBERSHIPS })],
  play: expectWorkspaceLayout,
};

export const ManyRows: Story = {
  decorators: [withData({ repos: MANY_REPOSITORIES })],
  play: async (context) => {
    await expectWorkspaceLayout(context);
    await expect(context.canvasElement.querySelectorAll('.workspace-grid__items > li').length).toBe(25);
    await expect(context.canvasElement.querySelector('gbt-pagination')).not.toBeNull();
  },
};

export const Empty: Story = {
  decorators: [withData({ repos: [], groups: [] })],
};

export const EmptyTab: Story = {
  decorators: [withData({ tab: 'starred', starred: [] })],
};

export const Loading: Story = {
  decorators: [withData({ list: () => NEVER, listMember: () => NEVER })],
};

export const LoadError: Story = {
  decorators: [withData({ list: () => throwError(() => new Error('500')) })],
};

export const CreateDialog: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const button = await waitFor(() => {
      const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouveau dépôt'));
      if (!found) throw new Error('"Nouveau dépôt" not rendered yet');
      return found;
    });
    button.click();
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('[role="dialog"]')).not.toBeNull());
  },
};

export const CreateGroupDialog: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const button = await waitFor(() => {
      const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouveau groupe'));
      if (!found) throw new Error('"Nouveau groupe" not rendered yet');
      return found;
    });
    button.click();
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('[role="dialog"]')?.textContent).toContain('Nouveau groupe'));
  },
};
