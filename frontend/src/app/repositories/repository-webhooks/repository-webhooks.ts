import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { CheckboxGroup, CheckboxGroupSection } from '@masmarino/gabarit/checkbox-group';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { Drawer } from '@masmarino/gabarit/drawer';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { RepositorySettingsService, WebhookDelivery, WebhookSummary, WEBHOOK_EVENT_OPTIONS } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';
import { TranslocoPipe } from '@jsverse/transloco';
import { t, tn } from '../../shared/i18n/translator';

type WebhookEvent = (typeof WEBHOOK_EVENT_OPTIONS)[number];

/** `label` is the short form inside a group (checkboxes), `full` the standalone one (rows, history). */
const eventGroups = (): { label: string; events: { value: WebhookEvent; label: string; full: string }[] }[] => [
  {
    label: t('repositories.webhooks.groupMergeRequests'),
    events: [
      { value: 'merge_request_approved', label: t('repositories.webhooks.mrApproved'), full: t('repositories.webhooks.mrApprovedFull') },
      { value: 'merge_request_changes_requested', label: t('repositories.webhooks.mrChanges'), full: t('repositories.webhooks.mrChanges') },
      { value: 'merge_request_commented', label: t('repositories.webhooks.mrCommented'), full: t('repositories.webhooks.mrCommentedFull') },
      { value: 'merge_request_merged', label: t('repositories.webhooks.mrMerged'), full: t('repositories.webhooks.mrMergedFull') },
      { value: 'merge_request_closed', label: t('repositories.webhooks.mrClosed'), full: t('repositories.webhooks.mrClosedFull') },
    ],
  },
  {
    label: t('repositories.webhooks.groupIssues'),
    events: [
      { value: 'issue_assigned', label: t('repositories.webhooks.issueAssigned'), full: t('repositories.webhooks.issueAssignedFull') },
      { value: 'issue_commented', label: t('repositories.webhooks.issueCommented'), full: t('repositories.webhooks.issueCommentedFull') },
      { value: 'issue_closed', label: t('repositories.webhooks.issueClosed'), full: t('repositories.webhooks.issueClosedFull') },
    ],
  },
  {
    label: t('repositories.webhooks.groupPipelines'),
    events: [
      { value: 'pipeline_failed', label: t('repositories.webhooks.pipelineFailed'), full: t('repositories.webhooks.pipelineFailedFull') },
    ],
  },
  {
    label: t('repositories.webhooks.groupCollaborators'),
    events: [
      { value: 'collaborator_added', label: t('repositories.webhooks.collaboratorAdded'), full: t('repositories.webhooks.collaboratorAddedFull') },
      { value: 'collaborator_role_changed', label: t('repositories.webhooks.roleChanged'), full: t('repositories.webhooks.roleChangedFull') },
      { value: 'collaborator_removed', label: t('repositories.webhooks.collaboratorRemoved'), full: t('repositories.webhooks.collaboratorRemovedFull') },
    ],
  },
];

function eventLabel(event: string): string {
  return eventGroups()
    .flatMap((group) => group.events)
    .find((candidate) => candidate.value === event)?.full ?? event;
}

const VISIBLE_EVENTS = 3;

@Component({
  selector: 'fg-repository-webhooks',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, GbtInput, Badge, Alert, EmptyState, Button, CheckboxGroup, ConfirmDangerModal, Drawer, Icon, Skeleton, SkeletonList, ListRow, Card, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './repository-webhooks.html',
  styleUrl: './repository-webhooks.scss',
})
export class RepositoryWebhooks implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected readonly skeletonRows = ['46%', '34%'];
  protected readonly eventSections: CheckboxGroupSection<WebhookEvent>[] = eventGroups().map((group) => ({
    label: group.label,
    options: group.events.map((event) => ({ value: event.value, label: event.label })),
  }));
  protected readonly eventLabel = eventLabel;

  protected list = createSettingsList(() => this.repositorySettings.listWebhooks(this.repositoryId()));
  protected newWebhookUrl = signal('');
  protected newWebhookSecret = signal('');
  protected newWebhookEvents = signal<WebhookEvent[]>([]);
  protected expandedWebhookId = signal<string | null>(null);
  protected expandedWebhookHeading = computed(() => {
    const webhook = this.list.items().find((w) => w.id === this.expandedWebhookId());
    return webhook ? t('repositories.webhooks.historyOf', { url: webhook.url }) : t('common.history');
  });
  protected deliveriesByWebhook = signal<Record<string, WebhookDelivery[] | undefined>>({});
  protected expandedDeliveries = computed(() => {
    const id = this.expandedWebhookId();
    return id === null ? null : (this.deliveriesByWebhook()[id] ?? null);
  });
  /** Webhook whose history failed to load, so the drawer can say so instead of spinning. */
  protected deliveriesFailedFor = signal<string | null>(null);
  protected webhookPendingDelete = signal<WebhookSummary | null>(null);
  /** Keeps the confirmation open and inert while the delete request runs. */
  protected deleting = signal(false);

  protected selectionSummary = computed(() => {
    const count = this.newWebhookEvents().length;
    return count === 0 ? t('repositories.webhooks.noneSelected') : tn('repositories.webhooks.selected', count);
  });

  protected rows = computed(() =>
    this.list.items().map((webhook) => {
      const labels = webhook.events.map(eventLabel);
      const hidden = labels.slice(VISIBLE_EVENTS);
      return { webhook, events: labels.slice(0, VISIBLE_EVENTS), more: hidden.length, moreTitle: hidden.join(', ') };
    }),
  );

  ngOnInit(): void {
    this.list.refresh();
  }

  addWebhook(): void {
    const url = this.newWebhookUrl().trim();
    const secret = this.newWebhookSecret();
    const events = this.newWebhookEvents();
    if (!url || !secret || events.length === 0) {
      return;
    }
    this.repositorySettings.createWebhook(this.repositoryId(), url, secret, events).subscribe({
      next: () => {
        this.newWebhookUrl.set('');
        this.newWebhookSecret.set('');
        this.newWebhookEvents.set([]);
        this.list.refresh();
        this.toast.show(t('repositories.webhooks.added'));
      },
      error: () => this.toast.show(t('repositories.webhooks.addFailed'), 'error'),
    });
  }

  confirmDeleteWebhook(webhook: WebhookSummary): void {
    this.webhookPendingDelete.set(webhook);
  }

  protected deleteWebhook(id: string): void {
    if (this.deleting()) {
      return;
    }
    this.deleting.set(true);
    this.repositorySettings.deleteWebhook(this.repositoryId(), id).subscribe({
      next: () => {
        this.deleting.set(false);
        this.webhookPendingDelete.set(null);
        this.list.refresh();
        this.toast.show(t('repositories.webhooks.deleted'));
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.deleting.set(false);
        this.webhookPendingDelete.set(null);
        this.toast.show(t('repositories.webhooks.deleteFailed'), 'error');
      },
    });
  }

  toggleDeliveries(id: string): void {
    if (this.expandedWebhookId() === id) {
      this.expandedWebhookId.set(null);
      return;
    }
    this.expandedWebhookId.set(id);
    this.deliveriesFailedFor.set(null);
    this.repositorySettings.listWebhookDeliveries(this.repositoryId(), id).subscribe({
      next: (deliveries) => this.deliveriesByWebhook.update((current) => ({ ...current, [id]: deliveries })),
      error: () => {
        this.deliveriesFailedFor.set(id);
        this.toast.show(t('repositories.webhooks.historyFailed'), 'error');
      },
    });
  }

  closeDeliveries(): void {
    this.expandedWebhookId.set(null);
  }
}
