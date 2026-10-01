import { Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Avatar, Badge, Button, Card, CardHeader, GbtDateTimePipe, GbtRelativeTimePipe, Icon, IconMarker, Textarea } from '@masmarino/gabarit';
import { Comment, ExcerptLine, TimelineThread } from '../merge-requests.service';
import { shortSha } from '../merge-request-events';

@Component({
  selector: 'fg-mr-thread-card',
  standalone: true,
  imports: [FormsModule, Avatar, Badge, Button, Card, CardHeader, Icon, IconMarker, Textarea, GbtDateTimePipe, GbtRelativeTimePipe],
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

  protected rootAuthorName = computed(() => this.thread().root.author?.username ?? 'Utilisateur supprimé');

  // A resolved thread starts collapsed. That state is local to the UI and not saved.
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
    return comment.author?.username ?? 'Utilisateur supprimé';
  }

  // The +/- prefix mirrors FileDiffView: a real character, so the added/removed distinction survives without colour.
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
