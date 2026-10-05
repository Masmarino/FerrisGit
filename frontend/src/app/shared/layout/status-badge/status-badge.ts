import { Component, computed, input } from '@angular/core';
import { Badge, BadgeVariant } from '@masmarino/gabarit/badge';
import { Issue } from '../../../issues/issues.service';
import { MergeRequestSummary } from '../../../merge-requests/merge-requests.service';
import { PipelineSummary } from '../../../pipelines/pipelines.service';
import { ReleaseStatus } from '../../../releases/releases.service';

export type StatusKind = 'issue' | 'merge-request' | 'pipeline' | 'release';

export interface StatusPresentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

// Issues: a ring that fills with progress; the label says it in words.
const ISSUE_STATUSES: Record<Issue['status'], StatusPresentation> = {
  todo: { label: 'À faire', variant: 'neutral', icon: 'circle-dot' },
  in_progress: { label: 'En cours', variant: 'info', icon: 'circle-half' },
  in_review: { label: 'En revue', variant: 'warning', icon: 'circle-three-quarters' },
  done: { label: 'Terminé', variant: 'success', icon: 'circle-check' },
};

const MERGE_REQUEST_STATUSES: Record<MergeRequestSummary['status'], StatusPresentation> = {
  open: { label: 'Ouverte', variant: 'info', icon: 'git-pull-request' },
  merged: { label: 'Fusionnée', variant: 'success', icon: 'git-merge' },
  closed: { label: 'Fermée', variant: 'neutral', icon: 'git-pull-request-closed' },
};

// Pipelines: one circled glyph per outcome, and only a failure is red.
const PIPELINE_STATUSES: Record<PipelineSummary['status'], StatusPresentation> = {
  pending: { label: 'En attente', variant: 'neutral', icon: 'clock' },
  running: { label: 'En cours', variant: 'info', icon: 'circle-play' },
  success: { label: 'Réussi', variant: 'success', icon: 'circle-check' },
  failed: { label: 'Échoué', variant: 'error', icon: 'circle-x' },
  canceled: { label: 'Annulé', variant: 'neutral', icon: 'circle-slash' },
};

const RELEASE_STATUSES: Record<ReleaseStatus, StatusPresentation> = {
  draft: { label: 'Brouillon', variant: 'neutral', icon: 'pencil' },
  prerelease: { label: 'Pré-version', variant: 'warning', icon: 'flask-conical' },
  published: { label: 'Publiée', variant: 'success', icon: 'tag' },
};

const STATUSES: Record<StatusKind, Record<string, StatusPresentation>> = {
  issue: ISSUE_STATUSES,
  'merge-request': MERGE_REQUEST_STATUSES,
  pipeline: PIPELINE_STATUSES,
  release: RELEASE_STATUSES,
};

const FALLBACK_ICONS: Record<StatusKind, string> = {
  issue: 'circle-dot',
  'merge-request': 'git-pull-request',
  pipeline: 'circle-dot',
  release: 'tag',
};

/** The one place mapping a status to its label, variant and icon. An unknown status shows as raw text, in neutral. */
export function statusPresentation(kind: StatusKind, status: string): StatusPresentation {
  const known = Object.hasOwn(STATUSES[kind], status) ? STATUSES[kind][status] : undefined;
  return known ?? { label: status, variant: 'neutral', icon: FALLBACK_ICONS[kind] };
}

/** A status as a `gbt-badge`. Not colour-only: the label carries the meaning and the icon is decoration. */
@Component({
  selector: 'fg-status-badge',
  standalone: true,
  imports: [Badge],
  templateUrl: './status-badge.html',
  styleUrl: './status-badge.scss',
})
export class StatusBadge {
  kind = input.required<StatusKind>();
  status = input.required<string>();

  protected presentation = computed(() => statusPresentation(this.kind(), this.status()));
}
