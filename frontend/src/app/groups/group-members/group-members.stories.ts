import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { ActivatedRoute, convertToParamMap, provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { GroupMembers } from './group-members';
import { GroupMember, GroupMembership, GroupsService } from '../groups.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, inShellContentArea } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout } from '../../shared/layout/settings-story-helpers';

const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

const MEMBERS: GroupMember[] = [
  { userId: 'u1', username: 'camille.martin', role: 'maintainer', createdAt: daysAgo(200) },
  { userId: 'u2', username: 'julien.dubois', role: 'contributor', createdAt: daysAgo(45) },
  { userId: 'u3', username: 'lea.bernard', role: 'reader', createdAt: daysAgo(3) },
];

function withData(options: { members?: GroupMember[]; role?: GroupMembership['role']; listMembers?: () => Observable<GroupMember[]> } = {}) {
  return moduleMetadata({
    providers: [
      { provide: ActivatedRoute, useValue: { snapshot: { paramMap: convertToParamMap({ id: 'group-1' }) } } },
      {
        provide: GroupsService,
        useValue: {
          listMembers: options.listMembers ?? (() => of(options.members ?? MEMBERS)),
          listMember: () => of([{ id: 'group-1', path: 'acme-france/produits', role: options.role ?? 'maintainer' }]),
          addMember: () => of(undefined),
          removeMember: () => of(undefined),
          setMemberRole: () => of(undefined),
        },
      },
      { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
    ],
  });
}

async function expectMembersLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  const rows = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLElement>('.group-members__rows gbt-list-row'));
    if (found.length === 0) throw new Error('rows not rendered yet');
    return found;
  });
  await expectSettingsLayout(canvasElement);
  await expectRows(canvasElement);
  if (canvasElement.ownerDocument.documentElement.clientWidth === 0) {
    return;
  }
  for (const row of rows) {
    const trailing = row.querySelector('.gbt-list-row__trailing')!.getBoundingClientRect();
    await expect(trailing.width, 'the role sits in the trailing column').toBeGreaterThan(0);
    await expect(Math.round(row.getBoundingClientRect().right - trailing.right), 'on the right edge').toBe(16);
  }
}

const meta: Meta<GroupMembers> = {
  title: 'Groups/GroupMembers',
  component: GroupMembers,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withApp, inShellContentArea],
};

export default meta;
type Story = StoryObj<GroupMembers>;

export const Populated: Story = {
  decorators: [withData()],
  play: expectMembersLayout,
};

export const ReadOnly: Story = {
  decorators: [withData({ role: 'reader' })],
  play: expectMembersLayout,
};

export const LongNames: Story = {
  decorators: [
    withData({
      members: [
        { userId: 'u1', username: 'maximilien.de-la-tour-d-auvergne-et-de-bouillon-lamarck', role: 'maintainer', createdAt: daysAgo(12) },
        ...MEMBERS,
      ],
    }),
  ],
  play: expectMembersLayout,
};

export const Empty: Story = {
  decorators: [withData({ members: [] })],
};

export const Loading: Story = {
  decorators: [withData({ listMembers: () => NEVER })],
};

export const LoadError: Story = {
  decorators: [withData({ listMembers: () => throwError(() => new Error('500')) })],
};
