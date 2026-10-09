import { Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { MarkdownOutlineEntry } from '../../shared/markdown-view/markdown-view';
import { Panel } from '@masmarino/gabarit/panel';
import { TranslocoPipe } from '@jsverse/transloco';

/** An outline is worth a panel from two headings on: one heading is no table of contents. */
export function hasOutline(entries: readonly MarkdownOutlineEntry[]): boolean {
  return entries.length >= 2;
}

/** The "Sur cette page" panel. Each entry links to `#user-content-…` on the current URL. The router does no anchor scrolling, so the click scrolls to the heading and focuses it itself. */
@Component({
  selector: 'fg-wiki-outline',
  standalone: true,
  imports: [TranslocoPipe, RouterLink, Panel],
  templateUrl: './wiki-outline.html',
  styleUrl: './wiki-outline.scss',
})
export class WikiOutline {
  entries = input.required<MarkdownOutlineEntry[]>();

  protected readonly here: readonly string[] = [];

  protected goTo(id: string): void {
    const heading = document.getElementById(id);
    if (!heading) {
      return;
    }
    const reduceMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    heading.scrollIntoView?.({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'start' });
    // Headings aren't focusable: -1 lets the next Tab continue from the section just reached.
    heading.setAttribute('tabindex', '-1');
    heading.focus({ preventScroll: true });
  }
}
