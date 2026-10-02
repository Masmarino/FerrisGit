import { ChangeDetectionStrategy, Component, computed, input, linkedSignal, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { Button, Disclosure, NavTab, NavTabs } from '@masmarino/gabarit';
import { DocsSearch } from '../docs-search/docs-search';
import { docsPageCommands, DocsIndex } from '../docs.service';

let nextNavId = 0;

/**
 * The docs' left column: the search field, then every section with its pages. Only the current section starts open.
 * On a narrow layout the sections fold behind a toggle and the search stays.
 */
@Component({
  selector: 'fg-docs-nav',
  standalone: true,
  imports: [RouterLink, Button, Disclosure, NavTab, NavTabs, DocsSearch],
  templateUrl: './docs-nav.html',
  styleUrl: './docs-nav.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DocsNav {
  index = input.required<DocsIndex>();
  section = input<string | null>(null);
  page = input<string | null>(null);

  protected readonly bodyId = `fg-docs-nav-${++nextNavId}`;
  protected navOpen = signal(false);

  // What the reader opened or closed by hand; forgotten on moving to another section.
  private toggled = linkedSignal<string | null, Record<string, boolean>>({ source: this.section, computation: () => ({}) });

  protected sections = computed(() =>
    this.index().sections.map((section) => {
      const current = section.slug === this.section();
      return {
        slug: section.slug,
        title: section.title,
        open: this.toggled()[section.slug] ?? current,
        pages: section.pages.map((page) => ({
          slug: page.slug,
          title: page.title,
          commands: docsPageCommands(section.slug, page.slug),
          current: current && page.slug === this.page(),
        })),
      };
    }),
  );

  protected setOpen(slug: string, open: boolean): void {
    this.toggled.update((toggled) => ({ ...toggled, [slug]: open }));
  }

  protected close(): void {
    this.navOpen.set(false);
  }
}
