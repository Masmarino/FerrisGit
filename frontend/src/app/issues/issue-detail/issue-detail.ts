import { Component, computed, effect, inject, input, OnInit, signal, TemplateRef, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Select } from '@masmarino/gabarit/select';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Tag } from '@masmarino/gabarit/tag';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { Issue, IssueComment, IssuesService, isClosed } from '../issues.service';
import { issueKindPresentation } from '../issue-kind';
import { labelSelectOptions, milestoneSelectOptions } from '../issue-filters';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { MeService } from '../../shell/me.service';
import { UserRef } from '../../shared/user-ref';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';
import { PageTitleService } from '../../shell/page-title.service';

interface DiscussionCard {
  id: string;
  authorName: string;
  byIssueAuthor: boolean;
  verb: string;
  createdAt: string;
  body: string;
  description: boolean;
}

const UNKNOWN_USER = 'Utilisateur inconnu';

@Component({
  selector: 'fg-issue-detail',
  standalone: true,
  imports: [
    FormsModule,
    Alert,
    Avatar,
    Badge,
    Button,
    Card,
    CardHeader,
    DescriptionList,
    Icon,
    IconMarker,
    Select,
    Skeleton,
    Tag,
    Textarea,
    MarkdownView,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageLayout,
    PageHeader,
    Panel,
    StatusBadge,
    UserChip,
  ],
  templateUrl: './issue-detail.html',
  styleUrl: './issue-detail.scss',
})
export class IssueDetail implements OnInit {
  repositoryId = input.required<string>();
  number = input.required<number>();

  private shellTitle = inject(PageTitleService);
  private issuesService = inject(IssuesService);
  private repositoryContext = inject(RepositoryContextService);
  private labelsService = inject(LabelsService);
  private milestonesService = inject(MilestonesService);
  private me = inject(MeService);
  private toast = inject(GbtToastService);

  protected issue = signal<Issue | null>(null);
  protected loadFailed = signal(false);
  protected comments = signal<IssueComment[]>([]);
  protected labels = signal<Label[]>([]);
  protected milestones = signal<Milestone[]>([]);
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  protected canWrite = computed(() => this.role() === 'owner' || this.role() === 'contributor' || this.role() === 'maintainer');

  protected pageTitle = computed(() => {
    const issue = this.issue();
    return issue ? `#${issue.number} ${issue.title}` : '';
  });
  protected kind = computed(() => issueKindPresentation(this.issue()?.kind ?? ''));

  private createdTemplate = viewChild.required<TemplateRef<unknown>>('createdTemplate');
  private closedTemplate = viewChild.required<TemplateRef<unknown>>('closedTemplate');

  protected dateEntries = computed<DescriptionListEntry[]>(() => {
    const i = this.issue();
    if (!i) return [];
    const entries: DescriptionListEntry[] = [{ term: 'Créé', value: this.createdTemplate() }];
    if (i.closedAt) entries.push({ term: 'Fermé', value: this.closedTemplate() });
    return entries;
  });
  protected closed = computed(() => {
    const issue = this.issue();
    return issue ? isClosed(issue) : false;
  });

  protected cards = computed<DiscussionCard[]>(() => {
    const issue = this.issue();
    if (!issue) {
      return [];
    }
    const authorId = issue.author?.id ?? null;
    const description: DiscussionCard = {
      id: 'description',
      authorName: issue.author?.username ?? UNKNOWN_USER,
      byIssueAuthor: authorId !== null,
      verb: 'a ouvert ce ticket',
      createdAt: issue.createdAt,
      body: issue.description.trim(),
      description: true,
    };
    return [
      description,
      ...this.comments().map((comment) => ({
        id: comment.id,
        authorName: comment.author?.username ?? UNKNOWN_USER,
        byIssueAuthor: authorId !== null && comment.author?.id === authorId,
        verb: 'a commenté',
        createdAt: comment.createdAt,
        body: comment.body,
        description: false,
      })),
    ];
  });

  protected draft = signal('');
  protected posting = signal(false);

  protected canAssignToMe = computed(() => this.canWrite() && this.me.id() !== '' && this.issue()?.assignee?.id !== this.me.id());

  // Stable reference: binding `labels.map(...)` in the template built a new array on every pass, so NgModel
  // re-applied the value and re-triggered detection forever.
  protected labelIds = computed(() => this.issue()?.labels.map((label) => label.id) ?? []);
  protected labelOptions = computed(() => labelSelectOptions(this.labels()));
  protected milestoneOptions = computed(() => milestoneSelectOptions(this.milestones(), 'Aucun milestone'));
  protected milestoneTitle = computed(() => {
    const id = this.issue()?.milestoneId;
    return id ? (this.milestones().find((milestone) => milestone.id === id)?.title ?? null) : null;
  });

  protected participants = computed<UserRef[]>(() => {
    const issue = this.issue();
    if (!issue) {
      return [];
    }
    const byId = new Map<string, UserRef>();
    for (const user of [issue.author, issue.assignee, ...this.comments().map((comment) => comment.author)]) {
      if (user && !byId.has(user.id)) {
        byId.set(user.id, user);
      }
    }
    return [...byId.values()];
  });

  protected readonly skeletonLines = ['92%', '78%', '64%'];

  constructor() {
    effect(() => this.shellTitle.set(this.pageTitle()));
  }

  ngOnInit(): void {
    this.load();
  }

  private load(): void {
    this.issuesService.detail(this.repositoryId(), this.number()).subscribe({
      next: (issue) => this.issue.set(issue),
      error: () => {
        this.loadFailed.set(true);
        this.toast.show('Impossible de charger ce ticket.', 'error');
      },
    });
    this.issuesService.listComments(this.repositoryId(), this.number()).subscribe({
      next: (comments) => this.comments.set(comments),
      error: () => this.toast.show('Impossible de charger ce ticket.', 'error'),
    });
    this.labelsService.listForRepository(this.repositoryId()).subscribe({
      next: (labels) => this.labels.set(labels),
      error: () => this.toast.show('Impossible de charger ce ticket.', 'error'),
    });
    this.milestonesService.listForRepository(this.repositoryId()).subscribe({
      next: (milestones) => this.milestones.set(milestones),
      error: () => this.toast.show('Impossible de charger ce ticket.', 'error'),
    });
  }

  protected onLabelsChange(labelIds: string[]): void {
    this.labelsService.setForIssue(this.repositoryId(), this.issue()!.number, labelIds).subscribe({
      next: (labels) => {
        this.issue.update((i) => (i ? { ...i, labels } : i));
        this.toast.show('Labels mis à jour.');
      },
      error: () => this.toast.show('Impossible de mettre à jour les labels.', 'error'),
    });
  }

  protected onMilestoneChange(milestoneId: string | null): void {
    const issue = this.issue();
    if (!issue) {
      return;
    }
    this.issuesService.update(this.repositoryId(), issue.number, issue.title, issue.description, issue.kind, milestoneId).subscribe({
      next: (updated) => {
        this.issue.set(updated);
        this.toast.show('Milestone mis à jour.');
      },
      error: () => this.toast.show('Impossible de mettre à jour le milestone.', 'error'),
    });
  }

  protected assignToMe(): void {
    const me = this.me.id();
    if (!me) {
      return;
    }
    this.issuesService.assign(this.repositoryId(), this.number(), me).subscribe({
      next: (updated) => {
        this.issue.set(updated);
        this.toast.show('Ticket assigné.');
      },
      error: () => this.toast.show('Impossible de vous assigner le ticket. Réessayez plus tard.', 'error'),
    });
  }

  // The draft is kept until the POST succeeds, so a failed request doesn't lose what was typed.
  addComment(): void {
    const body = this.draft().trim();
    if (!body || this.posting()) {
      return;
    }
    this.posting.set(true);
    this.issuesService.addComment(this.repositoryId(), this.number(), body).subscribe({
      next: () => {
        this.posting.set(false);
        this.clearDraft();
        this.toast.show('Commentaire ajouté.');
        this.issuesService.listComments(this.repositoryId(), this.number()).subscribe({
          next: (comments) => this.comments.set(comments),
          error: () => this.toast.show('Commentaire envoyé, mais impossible de recharger la liste.', 'error'),
        });
      },
      error: () => {
        this.posting.set(false);
        this.toast.show("Impossible d'envoyer le commentaire.", 'error');
      },
    });
  }

  private clearDraft(): void {
    this.draft.set('');
  }

  closeIssue(): void {
    this.issuesService.close(this.repositoryId(), this.number()).subscribe({
      next: (issue) => {
        this.issue.set(issue);
        this.toast.show('Ticket fermé.');
      },
      error: () => this.toast.show('Impossible de fermer le ticket.', 'error'),
    });
  }

  reopenIssue(): void {
    this.issuesService.reopen(this.repositoryId(), this.number()).subscribe({
      next: (issue) => {
        this.issue.set(issue);
        this.toast.show('Ticket rouvert.');
      },
      error: () => this.toast.show('Impossible de rouvrir le ticket.', 'error'),
    });
  }
}
