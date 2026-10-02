import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable, of, shareReplay, tap } from 'rxjs';

export interface DocsPageEntry {
  slug: string;
  title: string;
  description: string;
}

export interface DocsSection {
  slug: string;
  title: string;
  pages: DocsPageEntry[];
}

/** `docs/index.json`: the sections and their pages, in reading order. */
export interface DocsIndex {
  sections: DocsSection[];
}

/** A page with its neighbours in reading order, across sections. */
export interface DocsLocation {
  section: DocsSection;
  page: DocsPageEntry;
  previous: DocsLink | null;
  next: DocsLink | null;
}

export interface DocsLink {
  sectionTitle: string;
  title: string;
  commands: string[];
}

export const DOCS_ROOT = '/docs';

export function docsPageCommands(section: string, page: string): string[] {
  return [DOCS_ROOT, section, page];
}

/** Every page in reading order, each with its section. */
export function docsReadingOrder(index: DocsIndex): { section: DocsSection; page: DocsPageEntry }[] {
  return index.sections.flatMap((section) => section.pages.map((page) => ({ section, page })));
}

/** Where `/docs` leads: the first page of the first section, or `null` for an empty index. */
export function firstDocsPage(index: DocsIndex): string[] | null {
  const first = docsReadingOrder(index)[0];
  return first ? docsPageCommands(first.section.slug, first.page.slug) : null;
}

export function locateDocsPage(index: DocsIndex, section: string | null, page: string | null): DocsLocation | null {
  const order = docsReadingOrder(index);
  const at = order.findIndex((entry) => entry.section.slug === section && entry.page.slug === page);
  if (at < 0) {
    return null;
  }
  const link = (entry: (typeof order)[number] | undefined): DocsLink | null =>
    entry ? { sectionTitle: entry.section.title, title: entry.page.title, commands: docsPageCommands(entry.section.slug, entry.page.slug) } : null;
  return { ...order[at], previous: link(order[at - 1]), next: link(order[at + 1]) };
}

// The SPA fallback answers an unknown path with index.html, which isn't a Markdown page.
const looksLikeHtml = (text: string) => /^\s*<!doctype html|^\s*<html[\s>]/i.test(text);

/**
 * The product documentation, shipped as static files under `/docs/`. The index and each page are fetched once and
 * kept, since they only change with a new version of the app. A failure isn't kept, so it gets retried.
 */
@Injectable({ providedIn: 'root' })
export class DocsService {
  private http = inject(HttpClient);
  private index$: Observable<DocsIndex> | null = null;
  private pages = new Map<string, string>();

  index(): Observable<DocsIndex> {
    this.index$ ??= this.http.get<DocsIndex>(`${DOCS_ROOT}/index.json`).pipe(
      tap({ error: () => (this.index$ = null) }),
      shareReplay(1),
    );
    return this.index$;
  }

  /** The page's Markdown. A missing file fails with a 404, including when the server answers with the app shell. */
  page(section: string, page: string): Observable<string> {
    const key = `${section}/${page}`;
    const known = this.pages.get(key);
    if (known !== undefined) {
      return of(known);
    }
    return this.http.get(`${DOCS_ROOT}/${key}.md`, { responseType: 'text' }).pipe(
      map((text) => {
        if (looksLikeHtml(text)) {
          throw new HttpErrorResponse({ status: 404, statusText: 'Not Found', url: `${DOCS_ROOT}/${key}.md` });
        }
        return text;
      }),
      tap((text) => this.pages.set(key, text)),
    );
  }
}

export const isNotFound = (error: unknown) => error instanceof HttpErrorResponse && error.status === 404;
