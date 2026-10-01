import { Component, ElementRef, Injector, OnInit, afterNextRender, computed, inject, input, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { CdkDragDrop, DragDropModule } from '@angular/cdk/drag-drop';
import { CdkScrollable } from '@angular/cdk/scrolling';
import {
  Alert,
  Badge,
  Button,
  GbtInput,
  GbtToastService,
  Icon,
  Menu,
  MenuItem,
  PageHeader,
  PageLayout,
  Select,
  SelectOption,
  Skeleton,
  Tag,
  UserChip,
} from '@masmarino/gabarit';
import { Issue, IssuesService } from '../issues.service';
import { IssueKindPresentation, issueKindPresentation } from '../issue-kind';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';
import { PageTitleService } from '../../shell/page-title.service';

const STATUSES = ['todo', 'in_progress', 'in_review', 'done'] as const;
type Status = (typeof STATUSES)[number];
type Board = Record<Status, Issue[]>;

const emptyBoard = (): Board => ({ todo: [], in_progress: [], in_review: [], done: [] });

const laneId = (status: Status) => `column-${status}`;

interface MoveTarget {
  status: Status;
  presentation: StatusPresentation;
}

interface CardView {
  issue: Issue;
  link: string[];
  kind: IssueKindPresentation;
  milestoneTitle: string | null;
  commentsLabel: string;
  menuLabel: string;
  moveTargets: MoveTarget[];
}

interface LaneView {
  status: Status;
  id: string;
  headingId: string;
  presentation: StatusPresentation;
  issues: Issue[];
  cards: CardView[];
}

const MOVE_TARGETS: Record<Status, MoveTarget[]> = Object.fromEntries(
  STATUSES.map((status) => [status, STATUSES.filter((other) => other !== status).map((other) => ({ status: other, presentation: statusPresentation('issue', other) }))]),
) as Record<Status, MoveTarget[]>;

/** Where to insert a card in a lane's full list, given where it was dropped among the visible cards. */
function insertionIndex(full: Issue[], shown: Issue[], index: number): number {
  if (index < shown.length) {
    return full.indexOf(shown[index]);
  }
  return shown.length > 0 ? full.indexOf(shown[shown.length - 1]) + 1 : full.length;
}

@Component({
  selector: 'fg-issue-kanban',
  standalone: true,
  imports: [FormsModule, RouterLink, DragDropModule, CdkScrollable, PageLayout, PageHeader, UserChip, Alert, Badge, Button, GbtInput, Icon, Menu, MenuItem, Select, Skeleton, Tag],
  templateUrl: './issue-kanban.html',
  styleUrl: './issue-kanban.scss',
})
export class IssueKanban implements OnInit {
  private pageTitle = inject(PageTitleService);
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private router = inject(Router);
  private issuesService = inject(IssuesService);
  private labelsService = inject(LabelsService);
  private milestonesService = inject(MilestonesService);
  private repoContext = inject(RepositoryContextService);
  private toast = inject(GbtToastService);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  protected board = signal<Board>(emptyBoard());
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected labels = signal<Label[]>([]);
  protected milestones = signal<Milestone[]>([]);
  protected selectedLabelIds = signal<string[]>([]);
  protected selectedMilestoneId = signal<string | null>(null);
  protected search = signal('');
  protected announcement = signal('');

  protected readonly laneIds = STATUSES.map(laneId);
  /**
   * A touch drag starts only after a 250ms press, so a swipe that starts on a card scrolls the board
   * (or the page) instead of picking the card up. The mouse drags at once. Kept as one stable object.
   */
  protected readonly dragStartDelay = { touch: 250, mouse: 0 };
  protected readonly skeletonCards = [['72%', '48%'], ['58%'], ['66%', '40%']];

  protected canWrite = computed(() => {
    const role = this.repoContext.current()?.role;
    return role === 'owner' || role === 'contributor' || role === 'maintainer';
  });

  protected labelOptions = computed<SelectOption<string>[]>(() => this.labels().map((label) => ({ value: label.id, label: label.name, color: label.color })));
  protected milestoneOptions = computed<SelectOption<string | null>[]>(() => [
    { value: null, label: 'Tous les milestones' },
    ...this.milestones().map((milestone) => ({ value: milestone.id, label: milestone.title })),
  ]);
  private milestoneTitleById = computed(() => new Map(this.milestones().map((milestone) => [milestone.id, milestone.title])));

  private query = computed(() => this.search().trim().toLowerCase());
  private matches = (issue: Issue) => issue.title.toLowerCase().includes(this.query());

  protected lanes = computed<LaneView[]>(() => {
    const board = this.board();
    const query = this.query();
    const milestoneTitles = this.milestoneTitleById();
    return STATUSES.map((status) => {
      const issues = query ? board[status].filter(this.matches) : board[status];
      return {
        status,
        id: laneId(status),
        headingId: `issue-kanban-lane-${status}`,
        presentation: statusPresentation('issue', status),
        issues,
        cards: issues.map((issue) => ({
          issue,
          link: ['/repositories', ...this.path(), '-', 'issues', String(issue.number)],
          kind: issueKindPresentation(issue.kind),
          milestoneTitle: issue.milestoneId ? (milestoneTitles.get(issue.milestoneId) ?? null) : null,
          commentsLabel: `${issue.commentCount} ${issue.commentCount === 1 ? 'commentaire' : 'commentaires'}`,
          menuLabel: `Déplacer le ticket #${issue.number} vers`,
          moveTargets: MOVE_TARGETS[status],
        })),
      };
    });
  });

  private totalCount = computed(() => STATUSES.reduce((sum, status) => sum + this.board()[status].length, 0));
  private shownCount = computed(() => this.lanes().reduce((sum, lane) => sum + lane.issues.length, 0));
  protected summary = computed(() => {
    const total = this.totalCount();
    const noun = total === 1 ? 'ticket' : 'tickets';
    return this.query() ? `${this.shownCount()} ${noun} sur ${total}` : `${total} ${noun}`;
  });

  protected hasServerFilters = computed(() => this.selectedLabelIds().length > 0 || this.selectedMilestoneId() !== null);
  protected hasActiveFilters = computed(() => this.query() !== '' || this.hasServerFilters());
  protected isEmptyRepository = computed(() => !this.loading() && !this.loadFailed() && this.totalCount() === 0 && !this.hasServerFilters());

  ngOnInit(): void {
    this.pageTitle.set('Tickets');
    this.load();
    this.labelsService.listForRepository(this.repositoryId()).subscribe({ next: (labels) => this.labels.set(labels) });
    this.milestonesService.listForRepository(this.repositoryId()).subscribe({ next: (milestones) => this.milestones.set(milestones) });
  }

  private load(): void {
    this.issuesService.list(this.repositoryId(), { labelIds: this.selectedLabelIds(), milestoneId: this.selectedMilestoneId() ?? undefined }).subscribe({
      next: (issues) => {
        const board = emptyBoard();
        for (const issue of issues) {
          // An unknown status (a newer backend) has no lane: the card is left out rather than crash the board.
          board[issue.status]?.push(issue);
        }
        this.board.set(board);
        this.loading.set(false);
        this.loadFailed.set(false);
      },
      error: () => {
        this.loading.set(false);
        this.loadFailed.set(true);
        this.toast.show('Impossible de charger les tickets.', 'error');
      },
    });
  }

  protected onLabelFilterChange(labelIds: string[]): void {
    this.selectedLabelIds.set(labelIds);
    this.load();
  }

  protected onMilestoneFilterChange(milestoneId: string | null): void {
    this.selectedMilestoneId.set(milestoneId);
    this.load();
  }

  protected resetFilters(): void {
    const refetch = this.hasServerFilters();
    this.search.set('');
    this.selectedLabelIds.set([]);
    this.selectedMilestoneId.set(null);
    if (refetch) {
      this.load();
    }
  }

  /** A card dropped on a lane. The CDK indices count only the shown cards, which a search may have filtered, so place it next to the visible card it was dropped by. Moving to another lane saves the new status. */
  drop(event: CdkDragDrop<Issue[]>, target: Status): void {
    const issue = event.item.data as Issue;
    const origin = this.laneOf(issue.id);
    if (!origin || (event.previousContainer === event.container && event.previousIndex === event.currentIndex)) {
      return;
    }
    const shown = event.container.data.filter((candidate) => candidate.id !== issue.id);
    this.place(issue, origin, target, (full) => insertionIndex(full, shown, event.currentIndex));
  }

  /** The keyboard way to move a card (the drag gesture has no equivalent): it goes to the end of the target lane, is announced, and focus follows it. */
  protected moveTo(issue: Issue, target: Status): void {
    const origin = this.laneOf(issue.id);
    if (!origin || origin === target) {
      return;
    }
    this.place(issue, origin, target, (full) => full.length);
    this.announcement.set(`Ticket #${issue.number} déplacé vers ${statusPresentation('issue', target).label}`);
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>(`#${laneId(target)} [data-issue-id="${issue.id}"] .issue-kanban__card-link`)?.focus(), {
      injector: this.injector,
    });
  }

  private place(issue: Issue, origin: Status, target: Status, at: (fullWithoutCard: Issue[]) => number): void {
    const board = this.board();
    const originIndex = board[origin].findIndex((candidate) => candidate.id === issue.id);
    const moved: Issue = origin === target ? issue : { ...issue, status: target };
    const next: Board = { ...board, [origin]: board[origin].filter((candidate) => candidate.id !== issue.id) };
    const targetList = next[target].slice();
    targetList.splice(at(targetList), 0, moved);
    next[target] = targetList;
    this.board.set(next);

    if (origin === target) {
      return;
    }
    this.issuesService.updateStatus(this.repositoryId(), issue.number, target).subscribe({
      error: () => {
        // Only revert if the card is still where this move left it: a newer, overlapping move may
        // already have taken it elsewhere, and putting it back here would duplicate it.
        const current = this.board();
        if (current[target].some((candidate) => candidate.id === issue.id)) {
          const originList = current[origin].slice();
          originList.splice(Math.min(originIndex, originList.length), 0, { ...moved, status: origin });
          this.board.set({ ...current, [target]: current[target].filter((candidate) => candidate.id !== issue.id), [origin]: originList });
        }
        this.toast.show('Impossible de déplacer le ticket.', 'error');
      },
    });
  }

  private laneOf(issueId: string): Status | null {
    const board = this.board();
    return STATUSES.find((status) => board[status].some((candidate) => candidate.id === issueId)) ?? null;
  }

  openList(): void {
    this.router.navigate(['/repositories', ...this.path(), '-', 'issues']);
  }
}
