import { Component, computed, input } from '@angular/core';
import { Badge, GbtDateTimePipe, GbtRelativeTimePipe, IconMarker, Tag } from '@masmarino/gabarit';
import { TimelineEvent } from '../merge-requests.service';
import { eventMarker, eventSegments } from '../merge-request-events';

@Component({
  selector: 'fg-mr-system-note',
  standalone: true,
  imports: [Badge, IconMarker, Tag, GbtDateTimePipe, GbtRelativeTimePipe],
  templateUrl: './mr-system-note.html',
  styleUrl: './mr-system-note.scss',
})
export class MrSystemNote {
  event = input.required<TimelineEvent>();

  protected segments = computed(() => eventSegments(this.event()));
  protected marker = computed(() => eventMarker(this.event()));
}
