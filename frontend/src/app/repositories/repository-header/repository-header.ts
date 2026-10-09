import { Component, computed, DOCUMENT, inject, input, output, signal } from '@angular/core';
import { Router } from '@angular/router';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { RepositoriesService, Repository, StarResponse } from '../repositories.service';
import { BranchSwitcher } from '../branch-switcher/branch-switcher';
import { READ_ONLY_REPOSITORY } from '../read-only-repository';
import { t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

/** Id of the clone box on the overview; the "Cloner" action and the `#cloner` fragment lead there. */
export const CLONE_PANEL_ID = 'cloner';

/** Scrolls to the clone panel and focuses its copy button. False when the page has none. */
export function revealClonePanel(doc: Document): boolean {
  const panel = doc.getElementById(CLONE_PANEL_ID);
  if (!panel) {
    return false;
  }
  const reduceMotion = doc.defaultView?.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
  panel.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'center' });
  panel.querySelector<HTMLElement>('button')?.focus({ preventScroll: true });
  return true;
}

@Component({
  selector: 'fg-repository-header',
  standalone: true,
  imports: [TranslocoPipe, BranchSwitcher, Badge, Button, Skeleton, PageHeader],
  templateUrl: './repository-header.html',
  styleUrl: './repository-header.scss',
})
export class RepositoryHeader {
  repo = input.required<Repository | null>();
  ref = input.required<string>();
  /** Fires on every star change, optimistic first then confirmed or reverted, so the page can mirror the count. */
  starChange = output<StarResponse>();

  private repositories = inject(RepositoriesService);
  private router = inject(Router);
  private document = inject(DOCUMENT);
  protected readonly readOnly = inject(READ_ONLY_REPOSITORY);

  private starOverride = signal<StarResponse | null>(null);
  protected starCount = computed(() => this.starOverride()?.starCount ?? this.repo()?.starCount ?? 0);
  protected isStarred = computed(() => this.starOverride()?.isStarred ?? this.repo()?.isStarred ?? false);
  protected starText = computed(() => String(this.starCount()));
  /** Same label either way: aria-pressed carries the state, a flipping label would announce it twice. */
  protected starLabel = computed(() => t('repositories.favoritesCount', { count: this.starCount() }));

  protected toggleStar(): void {
    const r = this.repo();
    if (!r) return;
    const wasStarred = this.isStarred();
    const previousCount = this.starCount();
    this.setStar({ starCount: wasStarred ? previousCount - 1 : previousCount + 1, isStarred: !wasStarred });

    const request = wasStarred ? this.repositories.unstar(r.id) : this.repositories.star(r.id);
    request.subscribe({
      next: (response) => this.setStar({ starCount: response.starCount, isStarred: response.isStarred }),
      error: () => this.setStar({ starCount: previousCount, isStarred: wasStarred }),
    });
  }

  private setStar(state: StarResponse): void {
    this.starOverride.set(state);
    this.starChange.emit(state);
  }

  /** On the overview, reveals the clone panel. Elsewhere it goes to the root overview, not the ref's tree, since the ref may be what's missing. */
  protected showClonePanel(): void {
    if (revealClonePanel(this.document)) {
      return;
    }
    const r = this.repo();
    if (!r) return;
    void this.router.navigate(['/repositories', ...r.path], { fragment: CLONE_PANEL_ID });
  }
}
