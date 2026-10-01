import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { of } from 'rxjs';
import { userEvent, within } from 'storybook/test';
import { NotificationBell } from './notification-bell';
import { Notification, NotificationsService } from '../../notifications/notifications.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';

// `provideRouter` and `provideFerrisgitIcons` return EnvironmentProviders, so they go in `applicationConfig`, not `moduleMetadata`.
const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

function makeNotification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: 'n1',
    kind: 'merge_request_commented',
    repositoryOwner: 'camille',
    repositoryName: 'ferrisgit-web',
    actorUsername: 'julien',
    mergeRequestId: 'mr-1',
    mergeRequestTitle: 'Ajoute la connexion via SSO',
    pipelineId: null,
    commitSha: null,
    issueId: null,
    issueNumber: null,
    issueTitle: null,
    role: null,
    read: false,
    createdAt: '2026-09-24T08:15:00Z',
    ...overrides,
  };
}

const notifications: Notification[] = [
  makeNotification({ id: 'n1', kind: 'merge_request_commented', actorUsername: 'julien', createdAt: '2026-09-24T08:15:00Z' }),
  makeNotification({
    id: 'n2',
    kind: 'pipeline_failed',
    actorUsername: null,
    mergeRequestId: null,
    mergeRequestTitle: null,
    pipelineId: 'pl-42',
    commitSha: '3fa9c21d7be04a55',
    createdAt: '2026-09-24T07:40:00Z',
  }),
  makeNotification({
    id: 'n3',
    kind: 'issue_assigned',
    actorUsername: 'sophie',
    mergeRequestId: null,
    mergeRequestTitle: null,
    issueId: 'i-7',
    issueNumber: 7,
    issueTitle: 'Le clonage SSH échoue derrière un proxy',
    createdAt: '2026-09-23T16:05:00Z',
  }),
  makeNotification({
    id: 'n4',
    kind: 'merge_request_approved',
    actorUsername: 'sophie',
    mergeRequestId: 'mr-2',
    mergeRequestTitle: 'Corrige la pagination des issues',
    read: true,
    createdAt: '2026-09-22T13:20:00Z',
  }),
  makeNotification({
    id: 'n5',
    kind: 'collaborator_added',
    actorUsername: 'camille',
    mergeRequestId: null,
    mergeRequestTitle: null,
    role: 'maintainer',
    read: true,
    createdAt: '2026-09-20T09:00:00Z',
  }),
];

function fakeNotificationsService(unreadCount: number, list: Notification[]): Pick<NotificationsService, 'list' | 'unreadCount' | 'markRead' | 'markAllRead'> {
  return {
    list: () => of(list),
    unreadCount: () => of({ count: unreadCount }),
    markRead: () => of(undefined),
    markAllRead: () => of(undefined),
  };
}

function withNotifications(unreadCount: number, list: Notification[]) {
  return moduleMetadata({ providers: [{ provide: NotificationsService, useValue: fakeNotificationsService(unreadCount, list) }] });
}

async function openDropdown(canvasElement: HTMLElement): Promise<void> {
  const canvas = within(canvasElement);
  await userEvent.click(await canvas.findByRole('button', { name: /^Notifications/ }));
}

const meta: Meta<NotificationBell> = {
  title: 'Shell/NotificationBell',
  component: NotificationBell,
  tags: ['autodocs'],
  decorators: [withApp],
  // The dropdown is anchored to the end of the trigger, so leave room for it below and to the left.
  parameters: { layout: 'centered' },
};

export default meta;
type Story = StoryObj<NotificationBell>;

export const NoUnread: Story = {
  decorators: [withNotifications(0, [])],
};

export const SomeUnread: Story = {
  decorators: [withNotifications(3, notifications)],
};

export const OverNinetyNineUnread: Story = {
  decorators: [withNotifications(142, notifications)],
};

export const OpenWithNotifications: Story = {
  decorators: [withNotifications(3, notifications)],
  play: async ({ canvasElement }) => openDropdown(canvasElement),
};

export const OpenAllRead: Story = {
  decorators: [withNotifications(0, notifications.map((n) => ({ ...n, read: true })))],
  play: async ({ canvasElement }) => openDropdown(canvasElement),
};

export const OpenEmpty: Story = {
  decorators: [withNotifications(0, [])],
  play: async ({ canvasElement }) => openDropdown(canvasElement),
};
