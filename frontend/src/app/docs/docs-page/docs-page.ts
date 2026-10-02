import { ChangeDetectionStrategy, Component, computed, DestroyRef, effect, ElementRef, inject, signal } from '@angular/core';
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, ParamMap, RouterLink } from '@angular/router';
import { BehaviorSubject, catchError, combineLatest, debounceTime, map, Observable, of, startWith, switchMap, tap } from 'rxjs';
import { Alert, Breadcrumb, Button, Card, CardLink, EmptyState, PageLayout, Skeleton } from '@masmarino/gabarit';
import { PageTitleService } from '../../shell/page-title.service';
import { findAnchorTarget, MarkdownOutlineEntry, MarkdownView } from '../../shared/markdown-view/markdown-view';
import { hasOutline, WikiOutline } from '../../wiki/wiki-outline/wiki-outline';
import { DocsNav } from '../docs-nav/docs-nav';
import { DOCS_ROOT, DocsIndex, DocsLocation, docsPageCommands, DocsService, firstDocsPage, isNotFound, locateDocsPage } from '../docs.service';

type DocsView =
  | { kind: 'loading' }
  | { kind: 'failed' }
  | { kind: 'not-found'; firstPage: string[] | null }
  | { kind: 'page'; location: DocsLocation; content: string };

/** The callouts of the docs' Markdown: a quote whose first words are `**Note**` or `**Attention**`. */
const CALLOUTS: Record<string, string> = { note: 'note', attention: 'warning' };

const keyOf = (section: string | null, page: string | null) => `${section}/${page}`;

/**
 * One documentation page (`/docs/<section>/<page>`), in the public layout or the shell: the navigation, the page with
 * its breadcrumb and its neighbours, and the outline beside it on a wide screen. An unknown address gets a "not found"
 * inside the same frame.
 */
@Component({
  selector: 'fg-docs-page',
  standalone: true,
  imports: [RouterLink, Alert, Breadcrumb, Button, Card, CardLink, EmptyState, PageLayout, Skeleton, MarkdownView, WikiOutline, DocsNav],
  templateUrl: './docs-page.html',
  styleUrl: './docs-page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DocsPage {
  private docs = inject(DocsService);
  private route = inject(ActivatedRoute);
  private pageTitle = inject(PageTitleService);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);

  protected readonly docsRoot = DOCS_ROOT;
  protected readonly skeletonRows = ['38%', '92%', '84%', '96%', '60%'];

  private reload$ = new BehaviorSubject<void>(undefined);
  protected index = signal<DocsIndex | null>(null);
  private params = toSignal(this.route.paramMap, { requireSync: true });
  protected sectionSlug = computed(() => this.params().get('section'));
  protected pageSlug = computed(() => this.params().get('page'));

  protected view = toSignal(
    combineLatest([this.route.paramMap, this.reload$]).pipe(switchMap(([params]) => this.load(params))),
    { initialValue: { kind: 'loading' } as DocsView },
  );

  protected page = computed(() => {
    const view = this.view();
    return view.kind === 'page' ? view : null;
  });
  protected notFound = computed(() => {
    const view = this.view();
    return view.kind === 'not-found' ? view : null;
  });

  protected outline = signal<MarkdownOutlineEntry[]>([]);
  protected showOutline = computed(() => this.view().kind === 'page' && hasOutline(this.outline()));

  // The navigation the reader asked for (page and `#fragment`), handled once its page is on screen.
  private wanted: { key: string; fragment: string | null; moveFocus: boolean } | null = null;
  private renderedKey: string | null = null;

  constructor() {
    effect(() => {
      const view = this.view();
      this.pageTitle.set(view.kind === 'page' ? view.location.page.title : view.kind === 'not-found' ? 'Page introuvable' : 'Documentation');
    });

    // Params and fragment change one after the other within a navigation: wait for both. The first visit (the page
    // load) does not move the focus; later ones do, as a new page would.
    let first = true;
    combineLatest([this.route.paramMap, this.route.fragment])
      .pipe(debounceTime(0), takeUntilDestroyed(inject(DestroyRef)))
      .subscribe(([params, fragment]) => {
        this.wanted = { key: keyOf(params.get('section'), params.get('page')), fragment, moveFocus: !first };
        first = false;
        this.settle();
      });
  }

  private load(params: ParamMap): Observable<DocsView> {
    const section = params.get('section');
    const page = params.get('page');
    return this.docs.index().pipe(
      tap((index) => this.index.set(index)),
      switchMap((index): Observable<DocsView> => {
        const location = locateDocsPage(index, section, page);
        const notFound: DocsView = { kind: 'not-found', firstPage: firstDocsPage(index) };
        if (!location) {
          return of(notFound);
        }
        return this.docs.page(location.section.slug, location.page.slug).pipe(
          map((content): DocsView => ({ kind: 'page', location, content })),
          catchError((error: unknown) => of<DocsView>(isNotFound(error) ? notFound : { kind: 'failed' })),
        );
      }),
      catchError(() => of<DocsView>({ kind: 'failed' })),
      startWith<DocsView>({ kind: 'loading' }),
    );
  }

  protected retry(): void {
    this.reload$.next();
  }

  protected sectionLink(location: DocsLocation): string[] {
    return docsPageCommands(location.section.slug, location.section.pages[0].slug);
  }

  /** After each render of the page: the outline, the callouts, then the pending navigation. */
  protected onRendered(entries: MarkdownOutlineEntry[]): void {
    this.outline.set(entries);
    const content = this.content();
    if (!content) {
      return;
    }
    for (const quote of Array.from(content.querySelectorAll('blockquote'))) {
      const lead = quote.firstElementChild?.tagName === 'P' ? quote.firstElementChild.firstChild : null;
      const word = lead instanceof HTMLElement && lead.tagName === 'STRONG' ? (lead.textContent ?? '').trim().toLowerCase() : '';
      const kind = CALLOUTS[word];
      if (kind) {
        quote.setAttribute('data-callout', kind);
        quote.setAttribute('role', 'note');
      }
    }
    const view = this.view();
    this.renderedKey = view.kind === 'page' ? keyOf(view.location.section.slug, view.location.page.slug) : null;
    this.settle();
  }

  private content(): HTMLElement | null {
    return this.host.nativeElement.querySelector<HTMLElement>('.docs-page__content');
  }

  /** Scrolls to the `#fragment` (the outline's `user-content-…` id or the bare slug of a link), or to the top of a new page. */
  private settle(): void {
    const wanted = this.wanted;
    const content = this.content();
    if (!wanted || wanted.key !== this.renderedKey || !content) {
      return;
    }
    this.wanted = null;
    const reduceMotion = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
    const target = wanted.fragment ? findAnchorTarget(content, wanted.fragment) : null;
    if (target) {
      target.scrollIntoView?.({ behavior: wanted.moveFocus && !reduceMotion ? 'smooth' : 'auto', block: 'start' });
      if (wanted.moveFocus) {
        this.focus(target);
      }
      return;
    }
    if (wanted.moveFocus) {
      const heading = content.querySelector<HTMLElement>('h1');
      if (heading) {
        this.focus(heading);
      }
      this.host.nativeElement.ownerDocument.defaultView?.scrollTo?.({ top: 0 });
    }
  }

  // Headings are not focusable: -1 lets the next Tab continue from there.
  private focus(element: HTMLElement): void {
    if (element.tabIndex < 0) {
      element.setAttribute('tabindex', '-1');
    }
    element.focus({ preventScroll: true });
  }
}
