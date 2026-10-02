import { Component, OnInit, computed, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { Alert, Badge, Button, GbtInput, Icon, PageLayout, Skeleton } from '@masmarino/gabarit';
import { WikiList, WikiPageSummary, WikiService } from '../wiki.service';
import { sortedByTitle, wikiLink } from '../wiki-links';
import { RepositoryContextService } from '../../repositories/repository-context.service';

function normalise(value: string): string {
  return value.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/**
 * The wiki's frame shared by its three views: the "Pages" nav in `[page-nav]`, the view in the main column, its
 * `[page-aside]` re-projected. The shell loads the page list once per instance and hands it to the view through
 * `loaded`, so the index and creation form do not fetch it again.
 */
@Component({
  selector: 'fg-wiki-layout',
  standalone: true,
  imports: [FormsModule, RouterLink, Alert, Badge, Button, GbtInput, Icon, Skeleton, PageLayout],
  templateUrl: './wiki-layout.html',
  styleUrl: './wiki-layout.scss',
})
export class WikiLayout implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();
  currentSlug = input<string | null>(null);
  /** False on the creation form itself: "Nouvelle page" would lead back to where the user is. */
  offerCreate = input(true);

  loaded = output<WikiList>();
  loadFailed = output<void>();

  private wiki = inject(WikiService);
  private router = inject(Router);
  private repositoryContext = inject(RepositoryContextService);

  protected readonly skeletonRows = ['72%', '54%', '80%', '62%'];

  protected state = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected pages = signal<WikiPageSummary[]>([]);
  protected search = signal('');
  protected navOpen = signal(false);

  private role = computed(() => this.repositoryContext.current()?.role ?? null);
  protected canCreate = computed(() => this.role() === 'owner' || this.role() === 'maintainer' || this.role() === 'contributor');

  protected sortedPages = computed(() => sortedByTitle(this.pages()));
  protected filteredPages = computed(() => {
    const query = normalise(this.search().trim());
    if (!query) {
      return this.sortedPages();
    }
    return this.sortedPages().filter((page) => normalise(page.title).includes(query) || normalise(page.slug).includes(query));
  });
  protected rows = computed(() => this.filteredPages().map((page) => ({ page, link: wikiLink(this.path(), page.slug) })));

  ngOnInit(): void {
    this.reload();
  }

  reload(): void {
    this.wiki.list(this.repositoryId()).subscribe({
      next: (list) => {
        this.pages.set(list.pages);
        this.state.set('loaded');
        this.loaded.emit(list);
      },
      error: () => {
        this.state.set('failed');
        this.loadFailed.emit();
      },
    });
  }

  protected createPage(): void {
    void this.router.navigate(wikiLink(this.path(), 'new'));
  }
}
