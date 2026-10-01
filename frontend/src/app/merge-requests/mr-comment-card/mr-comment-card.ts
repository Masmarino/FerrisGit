import { Component, computed, input } from '@angular/core';
import { Avatar, Card, CardHeader, GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit';
import { UserRef } from '../merge-requests.service';

@Component({
  selector: 'fg-mr-comment-card',
  standalone: true,
  imports: [Avatar, Card, CardHeader, GbtDateTimePipe, GbtRelativeTimePipe],
  templateUrl: './mr-comment-card.html',
  styleUrl: './mr-comment-card.scss',
})
export class MrCommentCard {
  author = input<UserRef | null>(null);
  createdAt = input.required<string>();
  body = input.required<string>();
  verb = input('a commenté');
  badge = input<string | null>(null);

  /** A `null` author is an account an admin deleted: the comment outlives it. */
  protected authorName = computed(() => this.author()?.username ?? 'Utilisateur supprimé');
}
