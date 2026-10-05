import { ChangeDetectionStrategy, Component, inject, output, viewChild } from '@angular/core';
import { Router } from '@angular/router';
import { Autocomplete, AutocompleteSearchFn } from '@masmarino/gabarit/autocomplete';
import { DocsSearchHit, DocsSearchService } from '../docs-search.service';

/** The search field above the docs navigation. Pages load on first focus, since most visits never search. */
@Component({
  selector: 'fg-docs-search',
  standalone: true,
  imports: [Autocomplete],
  templateUrl: './docs-search.html',
  styleUrl: './docs-search.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DocsSearch {
  /** Emitted once a result is opened, so a folded navigation can close. */
  opened = output<DocsSearchHit>();

  private searchService = inject(DocsSearchService);
  private router = inject(Router);
  private field = viewChild.required(Autocomplete);

  protected readonly search: AutocompleteSearchFn<DocsSearchHit> = (query) => this.searchService.search(query);
  protected readonly label = (hit: DocsSearchHit) => hit.pageTitle;
  protected readonly announce = (count: number) => (count === 0 ? 'Aucun résultat' : `${count} résultat${count > 1 ? 's' : ''}`);

  protected prepare(): void {
    this.searchService.prepare();
  }

  protected open(hit: DocsSearchHit): void {
    // Empty the field, or it keeps the chosen title.
    this.field().writeValue(null);
    void this.router.navigate(hit.commands, { fragment: hit.fragment ?? undefined });
    this.opened.emit(hit);
  }
}
