import { Component, OnInit, computed, inject, input, signal, viewChild } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { IconMarker } from '@masmarino/gabarit/icon-marker';
import { ListCard, ListCardState } from '@masmarino/gabarit/list-card';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { WikiList, WikiPageSummary } from '../wiki.service';
import { sortedByTitle, wikiLink } from '../wiki-links';
import { WikiLayout } from '../wiki-layout/wiki-layout';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

@Component({
  selector: 'fg-wiki-page-list',
  standalone: true,
  imports: [TranslocoPipe, RouterLink, Alert, Button, Icon, IconMarker, ListCard, WikiLayout, PageHeader],
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
  // Public so the spec can call it.
  canManage = computed(() => this.role() === 'owner' || this.role() === 'maintainer' || this.role() === 'contributor');

  protected countLabel = computed(() => (this.pages().length === 1 ? '1 page' : `${this.pages().length} pages`));
  protected rows = computed(() =>
    sortedByTitle(this.pages()).map((page) => ({ page, link: wikiLink(this.path(), page.slug) })),
  );
  /** The index card's state. A failed load is the alert next to it, not a card state (see the template). */
  protected cardState = computed<ListCardState>(() => (this.state() === 'loading' ? 'loading' : this.pages().length === 0 ? 'empty' : 'ready'));

  ngOnInit(): void {
    this.pageTitle.set(t('nav.wiki'));
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
    void this.router.navigate(wikiLink(this.path(), 'new'));
  }
}
