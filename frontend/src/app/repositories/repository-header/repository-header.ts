import { Component, computed, DOCUMENT, inject, input, output, signal } from '@angular/core';
import { Router } from '@angular/router';
import { Badge, Button, PageHeader, Skeleton } from '@masmarino/gabarit';
import { RepositoriesService, Repository, StarResponse } from '../repositories.service';
import { BranchSwitcher } from '../branch-switcher/branch-switcher';
import { READ_ONLY_REPOSITORY } from '../read-only-repository';

/** Id of the clone box on the overview, where the header's `Cloner` action and the `#cloner` fragment lead. */
export const CLONE_PANEL_ID = 'cloner';

/** Scrolls the clone panel into view and focuses its copy button. Returns false when the page has none. */
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
  imports: [BranchSwitcher, Badge, Button, Skeleton, PageHeader],
  templateUrl: './repository-header.html',
  styleUrl: './repository-header.scss',
})
export class RepositoryHeader {
  repo = input.required<Repository | null>();
  ref = input.required<string>();
  /** Emitted on every star change (optimistic, then confirmed or reverted) so the page can show the same count elsewhere. */
  starChange = output<StarResponse>();

  private repositories = inject(RepositoriesService);
  private router = inject(Router);
  private document = inject(DOCUMENT);
  protected readonly readOnly = inject(READ_ONLY_REPOSITORY);

  private starOverride = signal<StarResponse | null>(null);
  protected starCount = computed(() => this.starOverride()?.starCount ?? this.repo()?.starCount ?? 0);
  protected isStarred = computed(() => this.starOverride()?.isStarred ?? this.repo()?.isStarred ?? false);
  protected starText = computed(() => String(this.starCount()));
  /** A constant name: `aria-pressed` carries the state (a label that flipped too would announce it twice, in opposite directions). */
  protected starLabel = computed(() => `Favoris (${this.starCount()})`);

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

  /** On the overview, reveals the `Cloner` panel. Elsewhere, goes to the root's overview rather than the ref's tree, since the ref may be what does not exist. */
  protected showClonePanel(): void {
    if (revealClonePanel(this.document)) {
      return;
    }
    const r = this.repo();
    if (!r) return;
    void this.router.navigate(['/repositories', ...r.path], { fragment: CLONE_PANEL_ID });
  }
}
