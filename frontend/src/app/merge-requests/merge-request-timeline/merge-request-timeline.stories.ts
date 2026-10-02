import type { Meta, StoryObj } from '@storybook/angular-vite';
import { userEvent, waitFor, within } from 'storybook/test';
import { MergeRequestTimeline } from './merge-request-timeline';
import { Comment, TimelineEvent, TimelineItem, TimelineThread } from '../merge-requests.service';
import { expectOnRail } from '../timeline-story-helpers';
import { daysAgo, hoursAgo, minutesAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { ALICE, BOB, CAROL, commentFixture, eventFixture, mergeRequestFixture } from '../merge-request-fixtures';

const mergeRequest = mergeRequestFixture({
  id: 'mr-1',
  title: 'Panier persistant',
  sourceBranch: 'feature/panier-persistant',
  createdAt: daysAgo(2),
  description: 'Conserve le panier du client entre deux sessions.\n\nÀ vérifier : la migration des paniers existants et l’expiration après 30 jours.',
});

function makeComment(overrides: Partial<Comment>): Comment {
  return commentFixture({
    id: 'c',
    author: BOB,
    body: '',
    createdAt: hoursAgo(20),
    filePath: 'src/cart/persistence.rs',
    lineNumber: 32,
    side: 'new',
    ...overrides,
  });
}

function makeEvent(id: string, createdAt: string, kind: TimelineEvent['kind'], actor: TimelineEvent['actor'], payload: Record<string, unknown> = {}): TimelineEvent {
  return eventFixture({ id, createdAt, kind, actor, payload });
}

const openThread: TimelineThread = {
  type: 'thread',
  id: 't-open',
  createdAt: hoursAgo(26),
  filePath: 'src/cart/persistence.rs',
  lineNumber: 32,
  endLine: null,
  side: 'new',
  outdated: false,
  resolved: false,
  resolvedBy: null,
  resolvedAt: null,
  excerpt: [
    { line: 31, kind: 'context', content: 'pub fn save(cart: &Cart) -> Result<()> {\n' },
    { line: 32, kind: 'added', content: '    let ttl = Duration::days(30);\n' },
    { line: 33, kind: 'added', content: '    store.put(cart.id, cart, ttl)\n' },
  ],
  root: makeComment({ id: 't-open', body: 'Cette durée de 30 jours devrait être configurable.', createdAt: hoursAgo(26) }),
  replies: [
    makeComment({ id: 't-open-r1', author: ALICE, replyToId: 't-open', body: 'Bien vu, je l’ajoute aux réglages du magasin.', createdAt: hoursAgo(5), filePath: null, lineNumber: null, side: null }),
  ],
};

const resolvedThread: TimelineThread = {
  type: 'thread',
  id: 't-done',
  createdAt: hoursAgo(4),
  filePath: 'src/cart/migration.rs',
  lineNumber: 8,
  endLine: null,
  side: 'new',
  outdated: false,
  resolved: true,
  resolvedBy: BOB,
  resolvedAt: hoursAgo(1),
  excerpt: [{ line: 8, kind: 'added', content: '    for cart in legacy_carts() {\n' }],
  root: makeComment({ id: 't-done', author: CAROL, body: 'Pense à traiter les paniers vides.', createdAt: hoursAgo(4), filePath: 'src/cart/migration.rs', lineNumber: 8, resolved: true }),
  replies: [],
};

const generalComment: TimelineItem = {
  type: 'comment',
  id: 'c-general',
  createdAt: daysAgo(1.5),
  author: CAROL,
  body: 'Super, ça règle le problème des paniers perdus après déconnexion.',
};

const events: TimelineEvent[] = [
  makeEvent('e-labels', daysAgo(2), 'labels_changed', ALICE, { added: [{ id: 'l1', name: 'panier', color: '#3366ff' }], removed: [] }),
  makeEvent('e-milestone', daysAgo(1.9), 'milestone_changed', ALICE, { from: null, to: 'Sprint 12' }),
  makeEvent('e-title', daysAgo(1.8), 'title_changed', ALICE, { from: 'Panier', to: 'Panier persistant' }),
  makeEvent('e-push', hoursAgo(4.5), 'commits_pushed', ALICE, { fromSha: '1a2b3c4d5e6f', toSha: '9f8e7d6c5b4a' }),
  makeEvent('e-changes', hoursAgo(4.2), 'review_submitted', BOB, { decision: 'changes_requested' }),
  makeEvent('e-approve', hoursAgo(3), 'review_submitted', CAROL, { decision: 'approved' }),
  makeEvent('e-merged', minutesAgo(25), 'merged', ALICE, { mergeCommitSha: 'c0ffee1234567' }),
  makeEvent('e-closed', minutesAgo(5), 'closed', BOB),
];

const fullItems: TimelineItem[] = [events[0], events[1], events[2], generalComment, openThread, events[3], events[4], resolvedThread, events[5], events[6], events[7]];

const meta: Meta<MergeRequestTimeline> = {
  title: 'MergeRequests/MergeRequestTimeline',
  component: MergeRequestTimeline,
  tags: ['autodocs'],
  decorators: [withFerrisgitIcons],
  args: {
    mergeRequest,
    author: ALICE,
    items: fullItems,
    canWrite: true,
  },
};

export default meta;
type Story = StoryObj<MergeRequestTimeline>;

/** Everything hung on the rail (card avatars, thread avatar and markers, system notes, composer) sits within 0.5px of its axis. */
const RAIL_MARKERS = '.mr-comment__avatar, .mr-thread__avatar, .mr-thread__marker, .mr-system-note__marker, .mr-timeline__composer-marker';

export const Full: Story = {
  play: async ({ canvasElement }) => expectOnRail(canvasElement, RAIL_MARKERS, '.mr-timeline__body'),
};

export const OnlyEvents: Story = {
  args: { items: events },
};

export const Empty: Story = {
  args: { items: [], mergeRequest: { ...mergeRequest, description: '' } },
};

export const ReadOnly: Story = {
  args: { canWrite: false },
};

export const DiscussionsFilter: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('radio', { name: 'Discussions' }));
  },
};

export const ResolvedThreadExpanded: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Afficher la discussion sur src/cart/migration.rs:8' }));
    await waitFor(() => expectOnRail(canvasElement, '.mr-thread__marker--card', '.mr-timeline__body'));
    expectOnRail(canvasElement, RAIL_MARKERS, '.mr-timeline__body');
  },
};

export const AllThreadsResolved: Story = {
  args: { items: [events[0], generalComment, resolvedThread, events[5]] },
};

export const OldDates: Story = {
  args: {
    mergeRequest: { ...mergeRequest, createdAt: '2026-01-12T08:00:00Z' },
    items: [
      makeEvent('e-old-labels', '2026-01-12T09:00:00Z', 'labels_changed', ALICE, { added: [{ id: 'l1', name: 'panier', color: '#3366ff' }], removed: [] }),
      { ...generalComment, createdAt: '2026-01-13T10:30:00Z' },
      makeEvent('e-old-approve', '2026-01-14T16:00:00Z', 'review_submitted', CAROL, { decision: 'approved' }),
    ],
  },
};
