import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core'
import { DOCUMENT } from '@angular/common'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { ActivatedRoute, RouterLink } from '@angular/router'
import { TranslocoPipe } from '@jsverse/transloco'
import { Button } from '@masmarino/gabarit'
import { injectActiveLang } from '../../i18n/active-lang'
import { usePageMeta } from '../../seo/page-meta'
import { CodeBlock } from '../../shared/code-block/code-block'
import { Glow } from '../../shared/glow.directive'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { APP_URL, DOCS } from '../../shared/links'
import { Reveal } from '../../shared/motion/reveal.directive'
import { PlanFigure } from '../../shared/plan-figure/plan-figure'
import { PlanHighlight } from '../../shared/plan-highlight'
import { PLAN_FRAGMENTS } from '../../shared/plan-links'
import { ScreenName, Screenshot } from './screenshot/screenshot'
import { CI_EXAMPLE } from '../../shared/snippets'
import { tabIndexForKey } from '../../shared/tabs-keyboard'
import { Window } from '../../shared/window/window'

// Section headings are anchors named feature-<id>; the circles of the plan link to them.
const ANCHOR_PREFIX = 'feature-'

interface FeatureSection {
  id: string
  bullets: number
  screen?: ScreenName
  /** The address shown in the window bar of the screenshot. */
  url?: string
}

@Component({
  selector: 'app-features',
  imports: [
    RouterLink,
    TranslocoPipe,
    Button,
    CodeBlock,
    Glow,
    InlineCodePipe,
    PlanFigure,
    Reveal,
    Screenshot,
    Window,
  ],
  templateUrl: './features.html',
  styleUrl: './features.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Features {
  private readonly document = inject(DOCUMENT)
  private readonly plan = inject(PlanHighlight)

  protected readonly lang = injectActiveLang()

  protected readonly appUrl = APP_URL
  protected readonly docs = DOCS

  // Domains with a screenshot are tabs; without JavaScript the bar is hidden and the panels follow one another.
  // Texts are under features.sections.<id> and features.tabs.items.<id>.
  protected readonly tabs: FeatureSection[] = [
    { id: 'repos', bullets: 5, screen: 'repository', url: 'git.example.com/acme/api' },
    {
      id: 'mrs',
      bullets: 5,
      screen: 'merge-request',
      url: 'git.example.com/acme/api/merge-requests/42',
    },
    { id: 'issues', bullets: 3, screen: 'issues', url: 'git.example.com/acme/api/issues/board' },
    {
      id: 'ci',
      bullets: 4,
      screen: 'pipeline',
      url: 'git.example.com/acme/api/pipelines/3f2a9c1e',
    },
    { id: 'public', bullets: 3, screen: 'explore', url: 'git.example.com/explore' },
    { id: 'docs', bullets: 3, screen: 'docs', url: 'git.example.com/docs/ci-cd/reference-yaml' },
  ]

  // Tiles below the tabs, of different sizes: what matters most gets the most room.
  protected readonly more: FeatureSection[] = [
    { id: 'accounts', bullets: 3 },
    { id: 'webhooks', bullets: 3 },
    { id: 'wikis', bullets: 2 },
    { id: 'tech', bullets: 4 },
  ]
  protected readonly missingItems = [1, 2, 3, 4, 5, 6, 7]
  protected readonly ciExample = CI_EXAMPLE

  /** Index of the visible panel; the prerendered page shows the first. */
  protected readonly active = signal(0)
  /** The tab roles are only added once scripts run; until then the panels are plain sections. */
  protected readonly enhanced = signal(false)
  /** Panels animate on a tab change, not on first display. */
  protected readonly animated = signal(false)

  /** The tab of the section the circle being pointed at on the plan leads to, which gets the same mark. */
  protected readonly linkedTab = computed(() => {
    const circle = this.plan.active()
    return circle === null ? null : PLAN_FRAGMENTS.features[circle - 1].replace(ANCHOR_PREFIX, '')
  })

  constructor() {
    usePageMeta('features')

    afterNextRender(() => {
      this.enhanced.set(true)
      this.openTabOf(this.document.defaultView?.location.hash.replace('#', ''))
    })
    // The circles of the plan link to a fragment of this page: a change of fragment opens the matching tab.
    inject(ActivatedRoute)
      .fragment.pipe(takeUntilDestroyed(inject(DestroyRef)))
      .subscribe((fragment) => this.openTabOf(fragment))
  }

  /** `/en/features/#feature-ci` opens the CI/CD tab. Other fragments are left to the browser. */
  private openTabOf(fragment: string | null | undefined): void {
    const target = fragment?.replace(ANCHOR_PREFIX, '')
    const index = this.tabs.findIndex((tab) => tab.id === target)
    if (index >= 0 && index !== this.active()) {
      this.animated.set(true)
      this.active.set(index)
    }
  }

  protected bulletKeys(section: FeatureSection): number[] {
    return Array.from({ length: section.bullets }, (_, index) => index + 1)
  }

  protected select(index: number, focus = false): void {
    if (index === this.active()) return
    this.animated.set(true)
    this.active.set(index)
    if (focus) this.document.getElementById(`tab-${this.tabs[index].id}`)?.focus()
  }

  protected onKeydown(event: KeyboardEvent, index: number): void {
    const next = tabIndexForKey(event.key, index, this.tabs.length)
    if (next === null) return
    event.preventDefault()
    this.select(next, true)
  }
}
