import { inject, Injectable } from '@angular/core';
import { catchError, firstValueFrom, forkJoin, map, of, switchMap } from 'rxjs';
import { headingSlug } from '../shared/markdown-view/markdown-view';
import { docsPageCommands, docsReadingOrder, DocsService } from './docs.service';

export const DOCS_SEARCH_LIMIT = 8;

export interface DocsSearchHit {
  sectionTitle: string;
  pageTitle: string;
  commands: string[];
  /** Id of the best matching heading (`user-content-…`), so the result lands on it. */
  fragment: string | null;
  heading: string | null;
  /** Body text around the first match, or the page's description when only the title or headings match. */
  excerpt: { before: string; match: string; after: string };
}

interface Field {
  text: string;
  /** Lower-cased, accents removed. */
  folded: string;
  /** Where each character of `folded` sits in `text`. */
  origin: number[];
}

interface Heading {
  id: string;
  field: Field;
}

interface Document {
  order: number;
  sectionTitle: string;
  description: string;
  commands: string[];
  title: Field;
  section: Field;
  headings: Heading[];
  body: Field;
}

/** Lower case without accents, character by character, keeping track of where each one came from. */
function fold(text: string): Field {
  let folded = '';
  const origin: number[] = [];
  let at = 0;
  for (const char of text) {
    const plain = char.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
    folded += plain;
    for (let i = 0; i < plain.length; i++) {
      origin.push(at);
    }
    at += char.length;
  }
  return { text, folded, origin };
}

export function searchTokens(query: string): string[] {
  return [...new Set(fold(query).folded.split(/[^\p{L}\p{N}]+/u).filter(Boolean))];
}

/** Inline Markdown down to its text: links and images keep their label, emphasis and code marks are dropped. */
function inlineText(line: string): string {
  return line
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/`+/g, '')
    .replace(/(\*\*|__|\*|~~)/g, '')
    .replace(/(^|\s)_(\S)/g, '$1$2')
    .replace(/(\S)_(\s|$)/g, '$1$2');
}

/**
 * Title, headings and body text of one page. Heading ids follow `fg-markdown-view`: `user-content-<slug>` on every
 * h1–h4, and `-2`, `-3`… on a repeated slug in document order.
 */
export function parseDocsPage(markdown: string): { headings: { id: string; text: string }[]; body: string } {
  const headings: { id: string; text: string }[] = [];
  const used = new Set<string>();
  const body: string[] = [];
  let fence: string | null = null;
  for (const line of markdown.split(/\r?\n/)) {
    const fenceMark = /^\s*(`{3,}|~{3,})/.exec(line)?.[1];
    if (fence !== null) {
      if (fenceMark && fenceMark[0] === fence[0] && fenceMark.length >= fence.length) {
        fence = null;
      } else {
        body.push(line);
      }
      continue;
    }
    if (fenceMark) {
      fence = fenceMark;
      continue;
    }
    const heading = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      const text = inlineText(heading[2]).replace(/\s+/g, ' ').trim();
      if (level <= 4) {
        const base = `user-content-${headingSlug(text)}`;
        let id = base;
        for (let n = 2; used.has(id); n++) {
          id = `${base}-${n}`;
        }
        used.add(id);
        if (level >= 2) {
          headings.push({ id, text });
        }
      }
      continue;
    }
    if (/^\s*\|?\s*:?-{3,}/.test(line)) {
      continue; // table separator row
    }
    body.push(inlineText(line.replace(/^\s*(>\s*)+/, '').replace(/^\s*([-*+]|\d+[.)])\s+/, '').replace(/\|/g, ' ')));
  }
  return { headings, body: body.join(' ').replace(/\s+/g, ' ').trim() };
}

const isWordStart = (folded: string, at: number) => at === 0 || !/[\p{L}\p{N}]/u.test(folded[at - 1]);

function count(field: Field, token: string): number {
  let found = 0;
  for (let at = field.folded.indexOf(token); at >= 0 && found < 5; at = field.folded.indexOf(token, at + token.length)) {
    found++;
  }
  return found;
}

function weight(field: Field, token: string, base: number): number {
  const at = field.folded.indexOf(token);
  if (at < 0) {
    return 0;
  }
  return isWordStart(field.folded, at) ? base * 1.5 : base;
}

const EXCERPT_BEFORE = 40;
const EXCERPT_LENGTH = 140;

function excerptAround(field: Field, token: string): DocsSearchHit['excerpt'] | null {
  const at = field.folded.indexOf(token);
  if (at < 0) {
    return null;
  }
  const start = field.origin[at];
  const end = at + token.length < field.origin.length ? field.origin[at + token.length] : field.text.length;
  let from = Math.max(0, start - EXCERPT_BEFORE);
  let to = Math.min(field.text.length, from + EXCERPT_LENGTH);
  // Cut on word boundaries at both ends.
  if (from > 0) {
    const space = field.text.indexOf(' ', from);
    from = space >= 0 && space < start ? space + 1 : from;
  }
  if (to < field.text.length) {
    const space = field.text.lastIndexOf(' ', to);
    to = space > end ? space : to;
  }
  return {
    before: (from > 0 ? '…' : '') + field.text.slice(from, start),
    match: field.text.slice(start, end),
    after: field.text.slice(end, to) + (to < field.text.length ? '…' : ''),
  };
}

/**
 * Search over the whole documentation, in the browser. Nothing is fetched until the first search (or `prepare()`),
 * then every page once, kept by `DocsService`. Every word of the query must appear in the page; a word in the title
 * ranks higher than one in a heading, and a heading higher than the text.
 */
@Injectable({ providedIn: 'root' })
export class DocsSearchService {
  private docs = inject(DocsService);
  private documents: Promise<Document[]> | null = null;

  /** Starts loading the pages before the first search; the field calls it on focus. */
  prepare(): void {
    this.load().catch(() => {});
  }

  private load(): Promise<Document[]> {
    this.documents ??= firstValueFrom(
      this.docs.index().pipe(
        switchMap((index) => {
          const order = docsReadingOrder(index);
          if (order.length === 0) {
            return of([]);
          }
          // A page that fails to load drops out of the results instead of failing the whole search.
          return forkJoin(
            order.map(({ section, page }, position) =>
              this.docs.page(section.slug, page.slug).pipe(
                map((markdown): Document => {
                  const parsed = parseDocsPage(markdown);
                  return {
                    order: position,
                    sectionTitle: section.title,
                    description: page.description,
                    commands: docsPageCommands(section.slug, page.slug),
                    title: fold(page.title),
                    section: fold(section.title),
                    headings: parsed.headings.map((heading) => ({ id: heading.id, field: fold(heading.text) })),
                    body: fold(parsed.body),
                  };
                }),
                catchError(() => of(null)),
              ),
            ),
          ).pipe(map((documents) => documents.filter((document): document is Document => document !== null)));
        }),
      ),
    ).catch((error: unknown) => {
      this.documents = null;
      throw error;
    });
    return this.documents;
  }

  async search(query: string): Promise<DocsSearchHit[]> {
    const tokens = searchTokens(query);
    if (tokens.length === 0) {
      return [];
    }
    const documents = await this.load();
    const scored: { score: number; order: number; hit: DocsSearchHit }[] = [];
    for (const document of documents) {
      let score = 0;
      let complete = true;
      for (const token of tokens) {
        const inTitle = weight(document.title, token, 12);
        const inSection = weight(document.section, token, 3);
        const inHeading = Math.max(0, ...document.headings.map((heading) => weight(heading.field, token, 5)));
        const inBody = count(document.body, token);
        if (inTitle + inSection + inHeading + inBody === 0) {
          complete = false;
          break;
        }
        score += inTitle + inSection + inHeading + inBody;
      }
      if (!complete) {
        continue;
      }
      // The heading with the most query words, the first one on a tie.
      let heading: Heading | null = null;
      let headingMatches = 0;
      for (const candidate of document.headings) {
        const matches = tokens.filter((token) => candidate.field.folded.includes(token)).length;
        if (matches > headingMatches) {
          heading = candidate;
          headingMatches = matches;
        }
      }
      const longest = [...tokens].sort((a, b) => b.length - a.length);
      const excerpt = longest.map((token) => excerptAround(document.body, token)).find((found) => found !== null) ?? {
        before: document.description,
        match: '',
        after: '',
      };
      scored.push({
        score,
        order: document.order,
        hit: {
          sectionTitle: document.sectionTitle,
          pageTitle: document.title.text,
          commands: document.commands,
          fragment: heading?.id ?? null,
          heading: heading?.field.text ?? null,
          excerpt,
        },
      });
    }
    return scored
      .sort((a, b) => b.score - a.score || a.order - b.order)
      .slice(0, DOCS_SEARCH_LIMIT)
      .map((entry) => entry.hit);
  }
}
