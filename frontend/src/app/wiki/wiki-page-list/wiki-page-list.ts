import { Component, OnInit, computed, inject, input, signal, viewChild } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { Alert, Button, Icon, IconMarker, ListCard, ListCardState, PageHeader } from '@masmarino/gabarit';
import { WikiList, WikiPageSummary } from '../wiki.service';
import { WikiLayout } from '../wiki-layout/wiki-layout';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';

@Component({
  selector: 'fg-wiki-page-list',
  standalone: true,
  imports: [RouterLink, Alert, Button, Icon, IconMarker, ListCard, WikiLayout, PageHeader],
  templateUrl: './wiki-page-list.html',
  styleUrl: './wiki-page-list.scss',
})
export class WikiPageList implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private router = inject(Router);
  private repositoryContext = inject(RepositoryContextService);
  private pageTitle = inject(PageTitleService);
  private layout = viewChild.required(WikiLayout);

  protected state = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected pages = signal<WikiPageSummary[]>([]);
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  // Not `protected` because the spec calls it directly.
  canManage = computed(() => this.role() === 'owner' || this.role() === 'maintainer' || this.role() === 'contributor');

  protected countLabel = computed(() => (this.pages().length === 1 ? '1 page' : `${this.pages().length} pages`));
  protected rows = computed(() =>
    [...this.pages()]
      .sort((a, b) => a.title.localeCompare(b.title))
      .map((page) => ({ page, link: this.wikiLink(page.slug) })),
  );
  /** The index card's state (a failed load is the alert next to it, not a card state: see the template). */
  protected cardState = computed<ListCardState>(() => (this.state() === 'loading' ? 'loading' : this.pages().length === 0 ? 'empty' : 'ready'));

  ngOnInit(): void {
    this.pageTitle.set('Wiki');
  }

  refresh(): void {
    this.layout().reload();
  }

  protected onLoaded(list: WikiList): void {
    this.pages.set(list.pages);
    this.state.set('loaded');
  }

  protected onLoadFailed(): void {
    this.state.set('failed');
  }

  protected createPage(): void {
    void this.router.navigate(this.wikiLink('new'));
  }

  private wikiLink(slug: string): string[] {
    return ['/repositories', ...this.path(), '-', 'wiki', slug];
  }
}
