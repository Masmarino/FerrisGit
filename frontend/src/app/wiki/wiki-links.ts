import { WikiPageSummary } from './wiki.service';

/** Route of the wiki, or of one of its pages and sub-pages (`slug`, `'edit'`). */
export function wikiLink(path: string[], ...rest: string[]): string[] {
  return ['/repositories', ...path, '-', 'wiki', ...rest];
}

/** What a page is called until its title has loaded: the slug, minus its hyphens. */
export function titleFromSlug(slug: string): string {
  return slug.replace(/-/g, ' ');
}

export function sortedByTitle(pages: WikiPageSummary[]): WikiPageSummary[] {
  return [...pages].sort((a, b) => a.title.localeCompare(b.title));
}
