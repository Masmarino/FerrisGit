import type { Meta, StoryObj } from '@storybook/angular-vite';
import { userEvent, waitFor, within } from 'storybook/test';
import { MrThreadCard } from './mr-thread-card';
import { Comment, TimelineThread } from '../merge-requests.service';
import { expectOnRail, hoursAgo, minutesAgo, onTimelineRail, withFerrisgitIcons } from '../timeline-story-helpers';

const alice = { id: 'u1', username: 'alice' };
const bob = { id: 'u2', username: 'bob' };

function makeComment(overrides: Partial<Comment> = {}): Comment {
  return {
    id: 't1',
    authorId: 'u2',
    author: bob,
    body: 'Pourquoi ne pas garder la durée de vie fixe à une heure ?',
    createdAt: hoursAgo(3),
    replyToId: null,
    filePath: 'src/auth/session.rs',
    lineNumber: 48,
    endLine: null,
    side: 'new',
    outdated: false,
    resolved: false,
    suggestedContent: null,
    appliedAt: null,
    appliedCommitSha: null,
    ...overrides,
  };
}

function makeThread(overrides: Partial<TimelineThread> = {}): TimelineThread {
  return {
    type: 'thread',
    id: 't1',
    createdAt: hoursAgo(3),
    filePath: 'src/auth/session.rs',
    lineNumber: 48,
    endLine: null,
    side: 'new',
    outdated: false,
    resolved: false,
    resolvedBy: null,
    resolvedAt: null,
    excerpt: [
      { line: 46, kind: 'context', content: 'impl Session {\n' },
      { line: 47, kind: 'removed', content: '    pub fn refresh(&mut self) {\n' },
      { line: 47, kind: 'added', content: '    pub fn refresh(&mut self, ttl: Duration) {\n' },
      { line: 48, kind: 'added', content: '        self.expires_at = Utc::now() + ttl;\n' },
    ],
    root: makeComment(),
    replies: [],
    ...overrides,
  };
}

const replies: Comment[] = [
  makeComment({ id: 't1-r1', author: alice, authorId: 'u1', replyToId: 't1', body: 'Elle doit désormais être configurable depuis les réglages admin.', createdAt: hoursAgo(2), filePath: null, lineNumber: null, side: null }),
  makeComment({ id: 't1-r2', author: bob, authorId: 'u2', replyToId: 't1', body: 'D’accord, merci pour la précision.', createdAt: minutesAgo(40), filePath: null, lineNumber: null, side: null }),
];

const resolvedThread = makeThread({
  resolved: true,
  resolvedBy: alice,
  resolvedAt: minutesAgo(10),
  root: makeComment({ resolved: true }),
  replies,
});

const meta: Meta<MrThreadCard> = {
  title: 'MergeRequests/MrThreadCard',
  component: MrThreadCard,
  tags: ['autodocs'],
  decorators: [withFerrisgitIcons, onTimelineRail],
  args: {
    thread: makeThread(),
    canWrite: true,
  },
};

export default meta;
type Story = StoryObj<MrThreadCard>;

export const Open: Story = {
  play: async ({ canvasElement }) => expectOnRail(canvasElement, '.mr-thread__avatar'),
};

export const OpenWithReplies: Story = {
  args: { thread: makeThread({ replies }) },
};

export const WithSuggestion: Story = {
  args: { thread: makeThread({ root: makeComment({ body: 'On peut simplifier ainsi.', suggestedContent: '        self.expires_at = Utc::now() + ttl.min(MAX_TTL);\n' }) }) },
};

export const AppliedSuggestion: Story = {
  args: {
    thread: makeThread({
      root: makeComment({
        body: 'On peut simplifier ainsi.',
        suggestedContent: '        self.expires_at = Utc::now() + ttl.min(MAX_TTL);\n',
        appliedAt: minutesAgo(15),
        appliedCommitSha: 'a1b2c3d',
      }),
    }),
  },
};

export const Outdated: Story = {
  args: { thread: makeThread({ outdated: true }) },
};

export const ResolvedCollapsed: Story = {
  args: { thread: resolvedThread },
  play: async ({ canvasElement }) => expectOnRail(canvasElement, '.mr-thread__marker'),
};

export const ResolvedExpanded: Story = {
  args: { thread: resolvedThread },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Afficher la discussion sur src/auth/session.rs:48' }));
    await waitFor(() => expectOnRail(canvasElement, '.mr-thread__marker--card'));
  },
};

export const ResolvedReadOnlyExpanded: Story = {
  args: { thread: resolvedThread, canWrite: false },
  play: ResolvedExpanded.play,
};

export const ResolvedByUnknown: Story = {
  args: { thread: { ...resolvedThread, resolvedBy: null } },
};

export const ReadOnly: Story = {
  args: { thread: makeThread({ replies, root: makeComment({ suggestedContent: '        self.expires_at = Utc::now() + ttl;\n' }) }), canWrite: false },
};

export const MultiLineRange: Story = {
  args: {
    thread: makeThread({
      lineNumber: 47,
      endLine: 49,
      root: makeComment({ lineNumber: 47, endLine: 49, body: 'Ce bloc entier mériterait un test.' }),
      excerpt: [
        { line: 47, kind: 'added', content: '    pub fn refresh(&mut self, ttl: Duration) {\n' },
        { line: 48, kind: 'added', content: '        self.expires_at = Utc::now() + ttl;\n' },
        { line: 49, kind: 'added', content: '    }\n' },
      ],
    }),
  },
};

export const LongCodeLine: Story = {
  args: {
    thread: makeThread({
      excerpt: [
        { line: 47, kind: 'context', content: '    pub fn refresh(&mut self, ttl: Duration) {\n' },
        { line: 48, kind: 'added', content: '        self.expires_at = Utc::now().checked_add_signed(ttl).ok_or_else(|| SessionError::Overflow { session_id: self.id.clone(), requested_ttl: ttl })?;\n' },
      ],
      root: makeComment({ body: 'Cette ligne est trop longue, on pourrait extraire la construction de l’erreur.' }),
    }),
  },
};

export const OutdatedResolvedExpanded: Story = {
  args: { thread: { ...resolvedThread, outdated: true } },
  play: ResolvedExpanded.play,
};
