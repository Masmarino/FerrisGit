import { Component, computed, input } from '@angular/core';
import { Badge, BadgeVariant } from '@masmarino/gabarit/badge';
import { Issue } from '../../../issues/issues.service';
import { MergeRequestSummary } from '../../../merge-requests/merge-requests.service';
import { PipelineSummary } from '../../../pipelines/pipelines.service';
import { ReleaseStatus } from '../../../releases/releases.service';
import { t } from '../../i18n/translator';

export type StatusKind = 'issue' | 'merge-request' | 'pipeline' | 'release';

export interface StatusPresentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

// Issues: a ring that fills with progress; the label says it in words.
const ISSUE_STATUSES: Record<Issue['status'], StatusPresentation> = {
  todo: { get label() { return t('status.issue.todo'); }, variant: 'neutral', icon: 'circle-dot' },
  in_progress: { get label() { return t('status.issue.in_progress'); }, variant: 'info', icon: 'circle-half' },
  in_review: { get label() { return t('status.issue.in_review'); }, variant: 'warning', icon: 'circle-three-quarters' },
  done: { get label() { return t('status.issue.done'); }, variant: 'success', icon: 'circle-check' },
};

const MERGE_REQUEST_STATUSES: Record<MergeRequestSummary['status'], StatusPresentation> = {
  open: { get label() { return t('status.mergeRequest.open'); }, variant: 'info', icon: 'git-pull-request' },
  merged: { get label() { return t('status.mergeRequest.merged'); }, variant: 'success', icon: 'git-merge' },
  closed: { get label() { return t('status.mergeRequest.closed'); }, variant: 'neutral', icon: 'git-pull-request-closed' },
};

// Pipelines: one circled glyph per outcome, and only a failure is red.
const PIPELINE_STATUSES: Record<PipelineSummary['status'], StatusPresentation> = {
  pending: { get label() { return t('status.pipeline.pending'); }, variant: 'neutral', icon: 'clock' },
  running: { get label() { return t('status.pipeline.running'); }, variant: 'info', icon: 'circle-play' },
  success: { get label() { return t('status.pipeline.success'); }, variant: 'success', icon: 'circle-check' },
  failed: { get label() { return t('status.pipeline.failed'); }, variant: 'error', icon: 'circle-x' },
  canceled: { get label() { return t('status.pipeline.canceled'); }, variant: 'neutral', icon: 'circle-slash' },
};

const RELEASE_STATUSES: Record<ReleaseStatus, StatusPresentation> = {
  draft: { get label() { return t('status.release.draft'); }, variant: 'neutral', icon: 'pencil' },
  prerelease: { get label() { return t('status.release.prerelease'); }, variant: 'warning', icon: 'flask-conical' },
  published: { get label() { return t('status.release.published'); }, variant: 'success', icon: 'tag' },
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
