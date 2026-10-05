import { Component, Injector, OnInit, TemplateRef, afterNextRender, computed, inject, input, signal, viewChild } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { WikiPageDetail as WikiPageDetailResponse, WikiRevision, WikiService } from '../wiki.service';
import { WikiLayout } from '../wiki-layout/wiki-layout';
import { titleFromSlug, wikiLink } from '../wiki-links';
import { WikiOutline, hasOutline } from '../wiki-outline/wiki-outline';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { MarkdownOutlineEntry, MarkdownView } from '../../shared/markdown-view/markdown-view';

const plural = (count: number, one: string, many: string) => (count === 1 ? `1 ${one}` : `${count} ${many}`);

const RECENT_REVISIONS = 3;

@Component({
  selector: 'fg-wiki-page-detail',
  standalone: true,
  imports: [
    RouterLink,
    Avatar,
    Button,
    Card,
    CardHeader,
    ConfirmDangerModal,
    DescriptionList,
    EmptyState,
    Skeleton,
    SkeletonList,
    MarkdownView,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageHeader,
    Panel,
    UserChip,
    WikiLayout,
    WikiOutline,
    Badge,
  ],
  templateUrl: './wiki-page-detail.html',
  styleUrl: './wiki-page-detail.scss',
})
export class WikiPageDetail implements OnInit {
  repositoryId = input.required<string>();
  slug = input.required<string>();
  showHistory = input.required<boolean>();
  path = input.required<string[]>();

  private wiki = inject(WikiService);
  private repositoryContext = inject(RepositoryContextService);
  private router = inject(Router);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private injector = inject(Injector);
  private fragmentHandled = false;

  protected page = signal<WikiPageDetailResponse | null>(null);
  protected revisions = signal<WikiRevision[]>([]);
  protected revisionsState = signal<'loading' | 'loaded' | 'failed'>('loading');
  // Public so the spec can call it.
  selectedRevisionContent = signal<string | null>(null);
  protected selectedRevisionSha = signal<string | null>(null);
  protected loadingRevisionSha = signal<string | null>(null);
  protected notFound = signal(false);
  protected outline = signal<MarkdownOutlineEntry[]>([]);
  protected confirmingDelete = signal(false);
  protected readonly historySkeletonRows = 3;
  protected readonly pageSkeleton = ['92%', '84%', '96%', '60%'];
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  canManage = computed(() => this.role() === 'owner' || this.role() === 'maintainer' || this.role() === 'contributor');
  canDelete = computed(() => this.role() === 'owner' || this.role() === 'maintainer');

  protected title = computed(() => this.page()?.title ?? titleFromSlug(this.slug()));
  protected historyTitle = computed(() => `Historique de ${this.title()}`);
  protected deleteMessage = computed(() => `La page « ${this.title()} » sera supprimée du wiki pour tout le monde. Tapez son titre pour confirmer.`);

  protected latest = computed<WikiRevision | null>(() => this.revisions()[0] ?? null);
  protected recentRevisions = computed(() => this.revisions().slice(0, RECENT_REVISIONS));
  protected revisionCountLabel = computed(() => plural(this.revisions().length, 'révision', 'révisions'));
  protected historyLinkLabel = computed(() => (this.revisions().length === 1 ? 'Voir la révision' : `Voir les ${this.revisions().length} révisions`));
  protected historyRows = computed(() => this.revisions().map((revision, index) => ({ revision, shortSha: revision.commitSha.slice(0, 7), current: index === 0 })));
  protected created = computed(() => this.revisions().at(-1) ?? null);
  private createdTemplate = viewChild.required<TemplateRef<unknown>>('createdTemplate');
  private modifiedTemplate = viewChild.required<TemplateRef<unknown>>('modifiedTemplate');
  protected dateEntries = computed<DescriptionListEntry[]>(() => {
    const entries: DescriptionListEntry[] = [];
    if (this.created()) entries.push({ term: 'Créée', value: this.createdTemplate() });
    if (this.latest()) entries.push({ term: 'Modifiée', value: this.modifiedTemplate() });
    return entries;
  });
  protected contributors = computed(() => {
    const counts = new Map<string, number>();
    for (const revision of [...this.revisions()].reverse()) {
      counts.set(revision.authorName, (counts.get(revision.authorName) ?? 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([name, count]) => ({ name, countLabel: plural(count, 'révision', 'révisions') }));
  });

  protected showOutline = computed(() => hasOutline(this.outline()));
  protected hasAside = computed(() => {
    if (this.notFound()) {
      return false;
    }
    if (this.showHistory()) {
      return this.revisions().length > 0;
    }
    return this.page() !== null && (this.showOutline() || this.revisions().length > 0);
  });

  protected indexLink = computed(() => wikiLink(this.path()));
  protected viewLink = computed(() => wikiLink(this.path(), this.slug()));
  protected historyLink = computed(() => [...this.viewLink(), 'history']);
  protected editLink = computed(() => [...this.viewLink(), 'edit']);

  ngOnInit(): void {
    this.pageTitle.set(this.showHistory() ? this.historyTitle() : this.title());
    // Page mode needs the history too (byline, "Historique" panel): one extra request, and the page still shows without
    // them if it fails. In history mode it is the page's content.
    this.wiki.revisions(this.repositoryId(), this.slug()).subscribe({
      next: (revisions) => {
        this.revisions.set(revisions);
        this.revisionsState.set('loaded');
      },
      error: () => {
        this.revisionsState.set('failed');
        if (this.showHistory()) {
          this.notFound.set(true);
        }
      },
    });
    if (!this.showHistory()) {
      this.load();
    }
  }

  private load(): void {
    this.wiki.detail(this.repositoryId(), this.slug()).subscribe({
      next: (page) => {
        this.page.set(page);
        this.pageTitle.set(page.title);
      },
      error: () => this.notFound.set(true),
    });
  }

  /** The rendered headings, for the outline. The first time, a `#user-content-…` in the URL scrolls to its heading (the router has no anchor scrolling). Later renders don't move the reader. */
  protected onOutline(entries: MarkdownOutlineEntry[]): void {
    this.outline.set(entries);
    if (this.fragmentHandled) {
      return;
    }
    this.fragmentHandled = true;
    const fragment = this.router.parseUrl(this.router.url).fragment;
    if (fragment) {
      afterNextRender(() => document.getElementById(fragment)?.scrollIntoView({ block: 'start' }), { injector: this.injector });
    }
  }

  viewRevision(commitSha: string): void {
    this.loadingRevisionSha.set(commitSha);
    this.wiki.revisionContent(this.repositoryId(), this.slug(), commitSha).subscribe({
      next: (result) => {
        this.loadingRevisionSha.set(null);
        this.selectedRevisionSha.set(commitSha);
        this.selectedRevisionContent.set(result.content);
      },
      error: () => {
        this.loadingRevisionSha.set(null);
        this.toast.show('Impossible de charger cette version', 'error');
      },
    });
  }

  protected toggleRevision(commitSha: string): void {
    if (this.selectedRevisionSha() === commitSha) {
      this.selectedRevisionSha.set(null);
      this.selectedRevisionContent.set(null);
      return;
    }
    this.viewRevision(commitSha);
  }

  deletePage(): void {
    if (this.page()) {
      this.confirmingDelete.set(true);
    }
  }

  confirmDelete(): void {
    const page = this.page();
    if (!page) return;
    this.wiki.delete(this.repositoryId(), this.slug(), page.headSha).subscribe({
      next: () => {
        this.confirmingDelete.set(false);
        this.toast.show('Page wiki supprimée.');
        void this.router.navigate(this.indexLink());
      },
      error: () => {
        this.confirmingDelete.set(false);
        this.toast.show('Impossible de supprimer la page', 'error');
      },
    });
  }

  protected cancelDelete(): void {
    this.confirmingDelete.set(false);
  }

  protected go(link: string[]): void {
    void this.router.navigate(link);
  }
}
