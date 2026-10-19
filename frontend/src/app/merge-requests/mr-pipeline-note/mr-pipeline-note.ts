import { Component, computed, inject, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { Badge } from '@masmarino/gabarit/badge';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { IconMarker, IconMarkerTone } from '@masmarino/gabarit/icon-marker';
import { TranslocoPipe } from '@jsverse/transloco';
import { TimelinePipeline } from '../merge-requests.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { pipelineLink } from '../../pipelines/pipeline-helpers';
import { StatusBadge, statusPresentation } from '../../shared/layout/status-badge/status-badge';

/** A merge request pipeline in the timeline: when it started, on which source commit, and how it is doing now. */
@Component({
  selector: 'fg-mr-pipeline-note',
  standalone: true,
  imports: [TranslocoPipe, RouterLink, Badge, IconMarker, StatusBadge, GbtDateTimePipe, GbtRelativeTimePipe],
  templateUrl: './mr-pipeline-note.html',
  styleUrl: '../mr-system-note/mr-system-note.scss',
})
export class MrPipelineNote {
  pipeline = input.required<TimelinePipeline>();

  private repositoryContext = inject(RepositoryContextService);

  protected marker = computed(() => {
    const presentation = statusPresentation('pipeline', this.pipeline().status);
    const tone: IconMarkerTone = presentation.variant === 'error' || presentation.variant === 'success' || presentation.variant === 'info' ? presentation.variant : 'neutral';
    return { icon: presentation.icon, tone };
  });
  protected shortSha = computed(() => this.pipeline().sourceSha.slice(0, 7));
  protected link = computed(() => {
    const path = this.repositoryContext.current()?.path;
    return path ? pipelineLink(path, this.pipeline().id) : null;
  });
}
