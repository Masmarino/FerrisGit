import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import {
  Alert,
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  GbtDateTimePipe,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  ListRow,
  PageHeader,
  PageLayout,
  Panel,
  Skeleton,
  SkeletonList,
  StatGrid,
  StatTile,
} from '@masmarino/gabarit';
import { DashboardResponse, DashboardService } from '../dashboard.service';
import { SearchIssueResult, SearchMergeRequestResult, SearchRepositoryRef } from '../../search/search.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { notificationLink, notificationQueryParams, notificationSentence } from '../../notifications/notification-display';
import { Notification } from '../../notifications/notifications.service';
import { issueKindPresentation } from '../../issues/issue-kind';
import { StatusBadge, StatusKind } from '../../shared/layout/status-badge/status-badge';

const EMPTY: DashboardResponse = { assignedIssues: [], authoredIssues: [], authoredMergeRequests: [], mergeRequestsToReview: [], activity: [] };

/** The API returns at most this many items per category: a full list reads as "20+". */
const CAP = 20;

type CategoryKey = 'assignedIssues' | 'authoredIssues' | 'authoredMergeRequests' | 'mergeRequestsToReview';

interface Category {
  key: CategoryKey;
  label: string;
  icon: string;
  kind: StatusKind;
}

const CATEGORIES: readonly Category[] = [
  { key: 'assignedIssues', label: 'Tickets assignés', icon: 'circle-dot', kind: 'issue' },
  { key: 'authoredIssues', label: 'Tickets créés', icon: 'pencil', kind: 'issue' },
  { key: 'authoredMergeRequests', label: 'Mes demandes de fusion', icon: 'git-pull-request', kind: 'merge-request' },
  { key: 'mergeRequestsToReview', label: 'À relire', icon: 'eye', kind: 'merge-request' },
];

interface TodoRow {
  id: string;
  kind: StatusKind;
  /** Null for a merge request (its API id is not a number worth showing). */
  number: string | null;
  title: string;
  link: string[];
  repositoryPath: string;
  repositoryLink: string[];
  createdAt: string;
  icon: string;
  iconLabel: string;
  status: string;
}

interface TodoGroup extends Category {
  headingId: string;
  count: string;
  counterValue: number;
  rows: TodoRow[];
}

interface ActivityRow {
  id: string;
  sentence: string;
  link: string[];
  queryParams: Readonly<Record<string, string>> | null;
  read: boolean;
  createdAt: string;
}

function countLabel(length: number): string {
  return length >= CAP ? `${CAP}+` : String(length);
}

/** Gabarit gap: the counter writes `N+` only above its `max`, so a full category is handed over as one past `max`. */
function counterValue(length: number): number {
  return length >= CAP ? CAP + 1 : length;
}

function repositoryFields(repository: SearchRepositoryRef): Pick<TodoRow, 'repositoryPath' | 'repositoryLink'> {
  return { repositoryPath: repository.path.join('/'), repositoryLink: ['/repositories', ...repository.path] };
}

function issueRow(item: SearchIssueResult): TodoRow {
  const kind = issueKindPresentation(item.kind);
  return {
    id: item.id,
    kind: 'issue',
    number: `#${item.number}`,
    title: item.title,
    link: ['/repositories', ...item.repository.path, '-', 'issues', String(item.number)],
    ...repositoryFields(item.repository),
    createdAt: item.createdAt,
    icon: kind.icon,
    iconLabel: kind.label,
    status: item.status,
  };
}

function mergeRequestRow(item: SearchMergeRequestResult): TodoRow {
  return {
    id: item.id,
    kind: 'merge-request',
    number: null,
    title: item.title,
    link: ['/repositories', ...item.repository.path, '-', 'merge-requests', item.id],
    ...repositoryFields(item.repository),
    createdAt: item.createdAt,
    icon: 'git-pull-request',
    iconLabel: 'Demande de fusion',
    status: item.status,
  };
}

/** No line break just inside « guillemets »: a narrow rail column otherwise leaves a lone "»". */
function keepGuillemetsAttached(sentence: string): string {
  return sentence.replaceAll('« ', '« ').replaceAll(' »', ' »');
}

function activityRow(n: Notification): ActivityRow {
  return {
    id: n.id,
    sentence: keepGuillemetsAttached(notificationSentence(n)),
    link: notificationLink(n),
    queryParams: notificationQueryParams(n),
    read: n.read,
    createdAt: n.createdAt,
  };
}

@Component({
  selector: 'fg-home-page',
  standalone: true,
  imports: [RouterLink, GbtDateTimePipe, GbtRelativeTimePipe, PageLayout, PageHeader, Panel, ListRow, StatusBadge, Alert, Badge, Button, Card, CardHeader, EmptyState, Icon, Skeleton, SkeletonList, StatGrid, StatTile],
  templateUrl: './home-page.html',
  styleUrl: './home-page.scss',
})
export class HomePage implements OnInit {
  private dashboard = inject(DashboardService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private me = inject(MeService);

  protected readonly cap = CAP;
  protected data = signal<DashboardResponse>(EMPTY);
  protected loading = signal(true);
  protected loadFailed = signal(false);

  protected username = this.me.username;

  protected todoGroups = computed<TodoGroup[]>(() => {
    const data = this.data();
    return CATEGORIES.map((category) => {
      const rows = category.kind === 'issue' ? (data[category.key] as SearchIssueResult[]).map(issueRow) : (data[category.key] as SearchMergeRequestResult[]).map(mergeRequestRow);
      return { ...category, headingId: `home-page-group-${category.key}`, count: countLabel(rows.length), counterValue: counterValue(rows.length), rows };
    });
  });

  protected nothingToDo = computed(() => this.todoGroups().every((group) => group.rows.length === 0));

  protected activityRows = computed<ActivityRow[]>(() => this.data().activity.map(activityRow));

  protected unreadLabel = computed(() => {
    const unread = this.activityRows().filter((row) => !row.read).length;
    if (unread === 0) {
      return null;
    }
    return unread === 1 ? '1 non lue' : `${unread} non lues`;
  });

  protected readonly activitySkeletons = [0, 1, 2];

  ngOnInit(): void {
    this.pageTitle.set('Accueil');
    this.load();
  }

  protected load(): void {
    this.loading.set(true);
    this.loadFailed.set(false);
    this.dashboard.get().subscribe({
      next: (d) => {
        this.data.set(d);
        this.loading.set(false);
      },
      error: () => {
        this.toast.show('Impossible de charger le tableau de bord. Réessayez plus tard.', 'error');
        this.loadFailed.set(true);
        this.loading.set(false);
      },
    });
  }
}
