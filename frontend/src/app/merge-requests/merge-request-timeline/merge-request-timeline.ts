import { Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Badge, Button, Card, IconMarker, SegmentedControl, SegmentedControlOption, Textarea } from '@masmarino/gabarit';
import { MergeRequestSummary, TimelineItem, UserRef } from '../merge-requests.service';
import { MrCommentCard } from '../mr-comment-card/mr-comment-card';
import { MrSystemNote } from '../mr-system-note/mr-system-note';
import { MrThreadCard } from '../mr-thread-card/mr-thread-card';

type TimelineFilter = 'all' | 'discussions' | 'activity';

@Component({
  selector: 'fg-merge-request-timeline',
  standalone: true,
  imports: [FormsModule, Badge, Button, Card, IconMarker, SegmentedControl, Textarea, MrCommentCard, MrSystemNote, MrThreadCard],
  templateUrl: './merge-request-timeline.html',
  styleUrl: './merge-request-timeline.scss',
})
export class MergeRequestTimeline {
  mergeRequest = input.required<MergeRequestSummary>();
  author = input<UserRef | null>(null);
  items = input.required<TimelineItem[]>();
  canWrite = input(false);

  commentAdded = output<string>();
  replyAdded = output<{ replyToId: string; body: string }>();
  resolveToggled = output<{ commentId: string; resolved: boolean }>();
  applySuggestionClicked = output<{ commentId: string }>();

  protected filterOptions: SegmentedControlOption<TimelineFilter>[] = [
    { value: 'all', label: 'Tout' },
    { value: 'discussions', label: 'Discussions' },
    { value: 'activity', label: 'Activité' },
  ];

  protected filter = signal<TimelineFilter>('all');

  protected visibleItems = computed(() => {
    const filter = this.filter();
    const items = this.items();
    if (filter === 'discussions') {
      return items.filter((item) => item.type !== 'event');
    }
    if (filter === 'activity') {
      return items.filter((item) => item.type === 'event');
    }
    return items;
  });

  protected showDescription = computed(() => this.filter() !== 'activity' && this.mergeRequest().description !== '');

  protected isEmpty = computed(() => !this.showDescription() && this.visibleItems().length === 0);

  protected threadCounts = computed(() => {
    let open = 0;
    let resolved = 0;
    for (const item of this.items()) {
      if (item.type === 'thread') {
        if (item.resolved) {
          resolved += 1;
        } else {
          open += 1;
        }
      }
    }
    return { open, resolved };
  });

  protected openBadge = computed(() => {
    const { open } = this.threadCounts();
    return `${open} ${open > 1 ? 'ouvertes' : 'ouverte'}`;
  });

  protected resolvedBadge = computed(() => {
    const { resolved } = this.threadCounts();
    return `${resolved} ${resolved > 1 ? 'résolues' : 'résolue'}`;
  });

  // Full sentence for screen readers; the badges alone ("2 ouvertes") lack their noun.
  protected counterText = computed(() => {
    const { open, resolved } = this.threadCounts();
    return `${open} ${open > 1 ? 'discussions ouvertes' : 'discussion ouverte'} · ${resolved} ${resolved > 1 ? 'résolues' : 'résolue'}`;
  });

  protected hasThreads = computed(() => {
    const { open, resolved } = this.threadCounts();
    return open + resolved > 0;
  });

  protected draft = signal('');

  // The draft survives the emit: only the page knows if the POST worked, and it calls clearDraft() on
  // success.
  protected submitComment(): void {
    const body = this.draft().trim();
    if (!body) {
      return;
    }
    this.commentAdded.emit(body);
  }

  clearDraft(): void {
    this.draft.set('');
  }
}
