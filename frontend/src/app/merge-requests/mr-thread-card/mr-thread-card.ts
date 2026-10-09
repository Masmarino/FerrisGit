import { Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { Textarea } from '@masmarino/gabarit/textarea';
import { Comment, ExcerptLine, TimelineThread } from '../merge-requests.service';
import { shortSha } from '../../repositories/commit-format';
import { authorName } from '../merge-request-presentation';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'fg-mr-thread-card',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, Avatar, Badge, Button, Card, CardHeader, Icon, IconMarker, Textarea, GbtDateTimePipe, GbtRelativeTimePipe],
  templateUrl: './mr-thread-card.html',
  styleUrl: './mr-thread-card.scss',
})
export class MrThreadCard {
  thread = input.required<TimelineThread>();
  canWrite = input(false);

  replyAdded = output<{ replyToId: string; body: string }>();

  resolveToggled = output<{ commentId: string; resolved: boolean }>();

  applySuggestionClicked = output<{ commentId: string }>();

  protected shortSha = shortSha;

  protected location = computed(() => {
    const { filePath, lineNumber, endLine } = this.thread();
    return endLine !== null ? `${filePath}:${lineNumber}-${endLine}` : `${filePath}:${lineNumber}`;
  });

  protected comments = computed<Comment[]>(() => [this.thread().root, ...this.thread().replies]);

  protected rootAuthorName = computed(() => authorName(this.thread().root.author));

  // A resolved thread starts collapsed. Not persisted.
  protected expanded = signal(false);

  protected collapsed = computed(() => this.thread().resolved && !this.expanded());

  protected toggleExpanded(): void {
    this.expanded.update((expanded) => !expanded);
  }

  protected toggleResolve(): void {
    const thread = this.thread();
    this.resolveToggled.emit({ commentId: thread.root.id, resolved: !thread.resolved });
  }

  protected authorName(comment: Comment): string {
    return authorName(comment.author);
  }

  // Same +/- prefix as FileDiffView, so added vs removed doesn't depend on colour.
  protected excerptText(line: ExcerptLine): string {
    const prefix = line.kind === 'added' ? '+' : line.kind === 'removed' ? '-' : ' ';
    return prefix + line.content.replace(/\r?\n$/, '');
  }

  protected replyBody = signal('');

  protected submitReply(): void {
    const body = this.replyBody().trim();
    if (!body) {
      return;
    }
    this.replyAdded.emit({ replyToId: this.thread().root.id, body });
    this.replyBody.set('');
  }
}
