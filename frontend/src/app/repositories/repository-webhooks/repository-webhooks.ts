import { Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  Alert,
  Badge,
  Button,
  Card,
  CheckboxGroup,
  CheckboxGroupSection,
  ConfirmDangerModal,
  Drawer,
  EmptyState,
  GbtDateTimePipe,
  GbtInput,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  ListRow,
  Skeleton,
  SkeletonList,
} from '@masmarino/gabarit';
import { RepositorySettingsService, WebhookDelivery, WebhookSummary, WEBHOOK_EVENT_OPTIONS } from '../repository-settings.service';
import { createSettingsList } from '../settings-list';

type WebhookEvent = (typeof WEBHOOK_EVENT_OPTIONS)[number];

/** `label` is the short form inside a group (checkboxes), `full` the standalone one (rows, history). */
const EVENT_GROUPS: { label: string; events: { value: WebhookEvent; label: string; full: string }[] }[] = [
  {
    label: 'Demandes de fusion',
    events: [
      { value: 'merge_request_approved', label: 'Approuvée', full: 'Demande de fusion approuvée' },
      { value: 'merge_request_changes_requested', label: 'Modifications demandées', full: 'Modifications demandées' },
      { value: 'merge_request_commented', label: 'Commentée', full: 'Demande de fusion commentée' },
      { value: 'merge_request_merged', label: 'Fusionnée', full: 'Demande de fusion fusionnée' },
      { value: 'merge_request_closed', label: 'Fermée', full: 'Demande de fusion fermée' },
    ],
  },
  {
    label: 'Tickets',
    events: [
      { value: 'issue_assigned', label: 'Assigné', full: 'Ticket assigné' },
      { value: 'issue_commented', label: 'Commenté', full: 'Ticket commenté' },
      { value: 'issue_closed', label: 'Fermé', full: 'Ticket fermé' },
    ],
  },
  {
    label: 'Pipelines',
    events: [{ value: 'pipeline_failed', label: 'Échoué', full: 'Pipeline échoué' }],
  },
  {
    label: 'Collaborateurs',
    events: [
      { value: 'collaborator_added', label: 'Ajouté', full: 'Collaborateur ajouté' },
      { value: 'collaborator_role_changed', label: 'Rôle modifié', full: 'Rôle de collaborateur modifié' },
      { value: 'collaborator_removed', label: 'Retiré', full: 'Collaborateur retiré' },
    ],
  },
];

const EVENT_LABELS = new Map<string, string>(EVENT_GROUPS.flatMap((group) => group.events.map((event) => [event.value, event.full] as const)));

function eventLabel(event: string): string {
  return EVENT_LABELS.get(event) ?? event;
}

const VISIBLE_EVENTS = 3;

@Component({
  selector: 'fg-repository-webhooks',
  standalone: true,
  imports: [FormsModule, GbtInput, Badge, Alert, EmptyState, Button, CheckboxGroup, ConfirmDangerModal, Drawer, Icon, Skeleton, SkeletonList, ListRow, Card, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './repository-webhooks.html',
  styleUrl: './repository-webhooks.scss',
})
export class RepositoryWebhooks implements OnInit {
  repositoryId = input.required<string>();

  private repositorySettings = inject(RepositorySettingsService);
  private toast = inject(GbtToastService);

  protected readonly skeletonRows = ['46%', '34%'];
  protected readonly eventSections: CheckboxGroupSection<WebhookEvent>[] = EVENT_GROUPS.map((group) => ({
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
    return webhook ? `Historique — ${webhook.url}` : 'Historique';
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
    if (count === 0) {
      return 'Aucun événement sélectionné';
    }
    return count === 1 ? '1 événement sélectionné' : `${count} événements sélectionnés`;
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
        this.toast.show('Webhook ajouté.');
      },
      error: () => this.toast.show("Impossible d'ajouter ce webhook (URL invalide ou pointant vers une adresse interdite).", 'error'),
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
        this.toast.show('Webhook supprimé.');
      },
      // Close the confirmation first, it covers the page and would hide the error.
      error: () => {
        this.deleting.set(false);
        this.webhookPendingDelete.set(null);
        this.toast.show('Impossible de supprimer ce webhook.', 'error');
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
        this.toast.show("Impossible de charger l'historique des livraisons.", 'error');
      },
    });
  }

  closeDeliveries(): void {
    this.expandedWebhookId.set(null);
  }
}
