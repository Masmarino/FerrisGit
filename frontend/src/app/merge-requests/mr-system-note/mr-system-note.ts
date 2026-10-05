import { Component, computed, input } from '@angular/core';
import { Badge } from '@masmarino/gabarit/badge';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { Tag } from '@masmarino/gabarit/tag';
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
