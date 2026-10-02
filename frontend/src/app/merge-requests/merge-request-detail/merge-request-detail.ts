import { Component, computed, inject, input, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Observable } from 'rxjs';
import { Comment, FileDiff, MergeRequestSummary, MergeRequestsService, ReviewSummary, TimelineResponse } from '../merge-requests.service';
import { PageTitleService } from '../../shell/page-title.service';
import { injectRepositoryPermissions } from '../../repositories/repository-role';
import { FileDiffView } from '../file-diff-view/file-diff-view';
import { MergeRequestTimeline } from '../merge-request-timeline/merge-request-timeline';
import { MrApprovalsPanel } from '../mr-approvals-panel/mr-approvals-panel';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import {
  Alert,
  Badge,
  BadgeVariant,
  Button,
  Card,
  CardHeader,
  GbtDateTimePipe,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  PageHeader,
  PageLayout,
  Panel,
  Select,
  SelectOption,
  Skeleton,
  Tab,
  Tabs,
  Tag,
  UserChip,
} from '@masmarino/gabarit';
import { UserRef } from '../../shared/user-ref';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';
import { mergeRequestEnd } from '../merge-request-presentation';
import { reviewStatus } from '../review-status';

interface FileChangePresentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

const FILE_CHANGES: Record<FileDiff['change'], FileChangePresentation> = {
  added: { label: 'Ajouté', variant: 'success', icon: 'plus' },
  modified: { label: 'Modifié', variant: 'info', icon: 'pencil' },
  deleted: { label: 'Supprimé', variant: 'error', icon: 'x' },
  binary: { label: 'Binaire', variant: 'neutral', icon: 'file' },
};

interface DiffEntry {
  file: FileDiff;
  directory: string;
  name: string;
  change: FileChangePresentation;
  comments: Comment[];
}

const OVERVIEW_TAB = 0;

const LOAD_ERROR = 'Impossible de charger cette demande de fusion.';
const COMMENT_ERROR = 'Impossible d’envoyer votre commentaire — cette ligne ne fait peut-être plus partie du diff. Rechargez et réessayez.';

@Component({
  selector: 'fg-merge-request-detail',
  standalone: true,
  imports: [
    FormsModule,
    FileDiffView,
    MergeRequestTimeline,
    MrApprovalsPanel,
    Alert,
    Badge,
    Button,
    Card,
    CardHeader,
    Icon,
    Select,
    Skeleton,
    Tab,
    Tabs,
    Tag,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageLayout,
    PageHeader,
    Panel,
    StatusBadge,
    UserChip,
  ],
  templateUrl: './merge-request-detail.html',
  styleUrl: './merge-request-detail.scss',
})
export class MergeRequestDetail implements OnInit {
  repositoryId = input.required<string>();
  mergeRequestId = input.required<string>();

  private mergeRequests = inject(MergeRequestsService);
  private pageTitle = inject(PageTitleService);
  private permissions = injectRepositoryPermissions();
  private labelsService = inject(LabelsService);
  private milestonesService = inject(MilestonesService);
  private toast = inject(GbtToastService);

  private timelineView = viewChild(MergeRequestTimeline);

  protected mergeRequest = signal<MergeRequestSummary | null>(null);
  protected loadFailed = signal(false);
  protected diffs = signal<FileDiff[]>([]);
  protected comments = signal<Comment[]>([]);
  protected timeline = signal<TimelineResponse>({ author: null, items: [] });
  protected mergeConflict = signal(false);
  protected reviewSummary = signal<ReviewSummary | null>(null);
  protected canWrite = this.permissions.canWrite;
  /** The server only lets the owner and Maintainers merge, so a Contributor gets no button. */
  protected canMerge = this.permissions.canMaintain;
  protected labels = signal<Label[]>([]);
  protected milestones = signal<Milestone[]>([]);

  protected author = computed<UserRef | null>(() => this.mergeRequest()?.author ?? this.timeline().author ?? null);
  protected branchesTitle = computed(() => {
    const mr = this.mergeRequest();
    return mr ? `Fusion de ${mr.sourceBranch} vers ${mr.targetBranch}` : '';
  });
  protected ended = computed(() => {
    const mr = this.mergeRequest();
    return mr ? mergeRequestEnd(mr) : null;
  });
  protected commentCountLabel = computed(() => {
    const count = this.comments().length;
    return count === 0 ? null : `${count} ${count === 1 ? 'commentaire' : 'commentaires'}`;
  });

  protected activeTab = signal(OVERVIEW_TAB);
  protected showAside = computed(() => this.activeTab() === OVERVIEW_TAB);

  protected review = computed(() => reviewStatus(this.reviewSummary()));

  protected diffEntries = computed<DiffEntry[]>(() => {
    const comments = this.comments();
    return this.diffs().map((file) => ({
      file,
      directory: file.path.slice(0, file.path.lastIndexOf('/') + 1),
      name: file.path.slice(file.path.lastIndexOf('/') + 1),
      change: FILE_CHANGES[file.change] ?? { label: file.change, variant: 'neutral', icon: 'file' },
      comments: comments.filter((comment) => comment.filePath === file.path),
    }));
  });
  protected diffSummary = computed(() => {
    const count = this.diffs().length;
    return `${count} ${count === 1 ? 'fichier modifié' : 'fichiers modifiés'}`;
  });

  // Computed because `labels.map(...)` in the template made a new array on every pass, so NgModel kept
  // re-applying the value and change detection never settled.
  protected labelIds = computed(() => this.mergeRequest()?.labels.map((label) => label.id) ?? []);
  protected labelOptions = computed<SelectOption<string>[]>(() => this.labels().map((label) => ({ value: label.id, label: label.name, color: label.color })));
  protected milestoneOptions = computed<SelectOption<string | null>[]>(() => [
    { value: null, label: 'Aucun milestone' },
    ...this.milestones().map((milestone) => ({ value: milestone.id, label: milestone.title })),
  ]);
  protected milestoneTitle = computed(() => {
    const id = this.mergeRequest()?.milestoneId;
    return id ? (this.milestones().find((milestone) => milestone.id === id)?.title ?? null) : null;
  });

  /** Everyone involved, once each: author, then commenters and reviewers in timeline order, then the remaining reviewers. Unresolvable users are skipped. */
  protected participants = computed<UserRef[]>(() => {
    const people: (UserRef | null)[] = [this.author()];
    for (const item of this.timeline().items) {
      if (item.type === 'comment') {
        people.push(item.author);
      } else if (item.type === 'thread') {
        people.push(item.root.author, ...item.replies.map((reply) => reply.author));
      } else if (item.kind === 'review_submitted') {
        people.push(item.actor);
      }
    }
    for (const review of this.reviewSummary()?.reviews ?? []) {
      people.push({ id: review.userId, username: review.username });
    }
    const byId = new Map<string, UserRef>();
    for (const person of people) {
      if (person && !byId.has(person.id)) {
        byId.set(person.id, person);
      }
    }
    return [...byId.values()];
  });

  protected readonly skeletonLines = ['92%', '78%', '64%'];

  ngOnInit(): void {
    this.mergeRequests.detail(this.mergeRequestId()).subscribe({
      next: (mr) => {
        this.mergeRequest.set(mr);
        this.pageTitle.set(mr.title);
      },
      error: () => {
        this.loadFailed.set(true);
        this.toast.show(LOAD_ERROR, 'error');
      },
    });
    this.loadInto(this.mergeRequests.diff(this.mergeRequestId()), this.diffs);
    this.reloadComments();
    this.reloadReviews();
    this.reloadTimeline();
    this.loadInto(this.labelsService.listForRepository(this.repositoryId()), this.labels);
    this.loadInto(this.milestonesService.listForRepository(this.repositoryId()), this.milestones);
  }

  private loadInto<T>(request: Observable<T>, target: { set(value: T): void }): void {
    request.subscribe({ next: (value) => target.set(value), error: () => this.toast.show(LOAD_ERROR, 'error') });
  }

  protected onLabelsChange(labelIds: string[]): void {
    this.labelsService.setForMergeRequest(this.mergeRequestId(), labelIds).subscribe({
      next: (labels) => {
        this.mergeRequest.update((mr) => (mr ? { ...mr, labels } : mr));
        this.reloadTimeline();
        this.toast.show('Labels mis à jour.');
      },
      error: () => this.toast.show('Impossible de mettre à jour les labels.', 'error'),
    });
  }

  protected onMilestoneChange(milestoneId: string | null): void {
    const mergeRequest = this.mergeRequest();
    if (!mergeRequest) {
      return;
    }
    this.mergeRequests.update(this.mergeRequestId(), mergeRequest.title, mergeRequest.description, milestoneId).subscribe({
      next: (updated) => {
        this.mergeRequest.set(updated);
        this.reloadTimeline();
        this.toast.show('Milestone mis à jour.');
      },
      error: () => this.toast.show('Impossible de mettre à jour le milestone.', 'error'),
    });
  }

  private reloadComments(): void {
    this.loadInto(this.mergeRequests.listComments(this.mergeRequestId()), this.comments);
  }

  private reloadTimeline(): void {
    this.loadInto(this.mergeRequests.timeline(this.mergeRequestId()), this.timeline);
  }

  // The timeline and the Modifications tab load separate payloads, so a change in one must refresh both.
  private reloadDiscussion(): void {
    this.reloadComments();
    this.reloadTimeline();
  }

  private reloadReviews(): void {
    this.loadInto(this.mergeRequests.listReviews(this.mergeRequestId()), this.reviewSummary);
  }

  protected onTimelineComment(body: string): void {
    this.mergeRequests.addComment(this.mergeRequestId(), body).subscribe({
      next: () => {
        // Only now is the draft cleared, so a failed POST doesn't lose what was typed.
        this.timelineView()?.clearDraft();
        this.reloadDiscussion();
        this.toast.show('Commentaire ajouté.');
      },
      error: () => this.toast.show('Impossible d’ajouter le commentaire.', 'error'),
    });
  }

  protected onCommentAdded(event: { filePath: string; lineNumber: number; endLine?: number; side: 'old' | 'new'; body: string; suggestedContent?: string }): void {
    this.mergeRequests
      .addComment(this.mergeRequestId(), event.body, { filePath: event.filePath, lineNumber: event.lineNumber, endLine: event.endLine, side: event.side, suggestedContent: event.suggestedContent })
      .subscribe({
        next: () => {
          this.reloadDiscussion();
          this.toast.show('Commentaire ajouté.');
        },
        error: () => this.toast.show(COMMENT_ERROR, 'error'),
      });
  }

  protected onReplyAdded(event: { replyToId: string; body: string }): void {
    this.mergeRequests.addComment(this.mergeRequestId(), event.body, { replyToId: event.replyToId }).subscribe({
      next: () => {
        this.reloadDiscussion();
        this.toast.show('Réponse ajoutée.');
      },
      error: () => this.toast.show(COMMENT_ERROR, 'error'),
    });
  }

  protected onResolveToggled(event: { commentId: string; resolved: boolean }): void {
    const call = event.resolved ? this.mergeRequests.resolveComment(this.mergeRequestId(), event.commentId) : this.mergeRequests.unresolveComment(this.mergeRequestId(), event.commentId);
    call.subscribe({
      next: () => {
        this.reloadDiscussion();
        this.toast.show(event.resolved ? 'Commentaire résolu.' : 'Commentaire non résolu.');
      },
      error: () => this.toast.show(COMMENT_ERROR, 'error'),
    });
  }

  protected onApplySuggestionClicked(event: { commentId: string }): void {
    this.mergeRequests.applySuggestion(this.mergeRequestId(), event.commentId).subscribe({
      next: () => {
        this.reloadDiscussion();
        this.toast.show('Suggestion appliquée.');
      },
      error: () => this.toast.show(COMMENT_ERROR, 'error'),
    });
  }

  protected submitReview(decision: 'approved' | 'changes_requested'): void {
    this.mergeRequests.submitReview(this.mergeRequestId(), decision).subscribe({
      next: () => {
        this.reloadReviews();
        this.reloadTimeline();
        this.toast.show(decision === 'approved' ? 'Revue envoyée : approuvée.' : 'Revue envoyée : changements demandés.');
      },
      error: () => this.toast.show('Impossible d’envoyer votre revue.', 'error'),
    });
  }

  protected merge(): void {
    this.mergeConflict.set(false);
    this.mergeRequests.merge(this.mergeRequestId()).subscribe({
      next: (result) => {
        if (result.outcome === 'conflicting') {
          this.mergeConflict.set(true);
          return;
        }
        this.mergeRequest.set(result);
        this.reloadTimeline();
        this.toast.show('Demande de fusion fusionnée.');
      },
      error: () => this.toast.show('Une erreur est survenue pendant la fusion.', 'error'),
    });
  }

  protected close(): void {
    // Refetch instead of navigating away, so the status badge updates in place.
    this.mergeRequests.close(this.mergeRequestId()).subscribe({
      next: () => {
        this.mergeRequests.detail(this.mergeRequestId()).subscribe({ next: (mr) => this.mergeRequest.set(mr) });
        this.reloadTimeline();
        this.toast.show('Demande de fusion fermée.');
      },
      error: () => this.toast.show('Impossible de fermer la demande de fusion.', 'error'),
    });
  }
}
