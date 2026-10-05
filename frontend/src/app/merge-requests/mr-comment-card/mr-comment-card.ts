import { Component, computed, input } from '@angular/core';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { authorName } from '../merge-request-presentation';
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

  protected authorName = computed(() => authorName(this.author()));
}
