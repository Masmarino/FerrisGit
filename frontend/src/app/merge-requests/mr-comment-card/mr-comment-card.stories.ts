import type { Meta, StoryObj } from '@storybook/angular-vite';
import { MrCommentCard } from './mr-comment-card';
import { daysAgo, hoursAgo, minutesAgo } from '../../shared/layout/page-story-helpers';
import { expectOnRail, onTimelineRail } from '../timeline-story-helpers';

const meta: Meta<MrCommentCard> = {
  title: 'MergeRequests/MrCommentCard',
  component: MrCommentCard,
  tags: ['autodocs'],
  decorators: [onTimelineRail],
  args: {
    author: { id: 'u1', username: 'alice' },
    createdAt: hoursAgo(3),
    body: 'Bonne idée de passer à DateTime<Utc>, cela évite les conversions en secondes partout.',
    verb: 'a commenté',
    badge: null,
  },
  play: async ({ canvasElement }) => expectOnRail(canvasElement, '.mr-comment__avatar'),
};

export default meta;
type Story = StoryObj<MrCommentCard>;

export const Default: Story = {};

export const Description: Story = {
  args: {
    verb: 'a ouvert cette demande de fusion',
    badge: 'Auteur',
    createdAt: daysAgo(2),
    body: 'Cette demande de fusion remplace les horodatages en secondes par des DateTime<Utc>.\n\nÀ vérifier : la migration des sessions existantes.',
  },
};

export const UnknownAuthor: Story = {
  args: { author: null },
};

export const JustNow: Story = {
  args: { createdAt: minutesAgo(0) },
};

export const OldComment: Story = {
  args: { createdAt: '2026-01-05T09:30:00Z' },
};

export const LongUnbrokenText: Story = {
  args: { body: 'Voir https://example.com/une/tres/longue/adresse/sans/espace/qui/ne/doit/pas/deborder/de/la/carte/meme/sur/un/telephone/etroit' },
};
