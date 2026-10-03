import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  inject,
  signal,
  viewChild,
} from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { JobStatus } from '@masmarino/gabarit'
import { PlanHighlight, PlanPoint } from '../plan-highlight'

/**
 * What FerrisGit does, as a carousel of tiles that each show a piece of the interface. The track is a plain scroll
 * container with scroll snapping, so touch, trackpad and keyboard scrolling work without script; the buttons and dots
 * only move it. A link from the architecture plan to a tile (`#d2`…) scrolls the track to it like any anchor. The facts come from the docs
 * (docs/utilisation). Seven tiles carry the number of the architecture-plan circle that leads to them (ids d1 to d7).
 */
import { Reveal } from '../motion/reveal.directive'

@Component({
  selector: 'app-capabilities',
  imports: [Reveal, TranslocoPipe, JobStatus, PlanPoint],
  templateUrl: './capabilities.html',
  styleUrl: './capabilities.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Capabilities {
  protected readonly plan = inject(PlanHighlight)

  private readonly track = viewChild.required<ElementRef<HTMLElement>>('track')

  /** The tiles in the order of the circles of the plan that lead to them (1 to 7), the one with no circle last. */
  protected readonly slides = [
    'roles',
    'review',
    'kanban',
    'catalog',
    'search',
    'pipeline',
    'webhook',
    'mfa',
  ] as const

  /** The tile at the start edge of the track, or the last one once the track is scrolled to its end. */
  protected readonly current = signal(0)

  /** Whether the suggestion of the review tile has been applied. */
  protected readonly applied = signal(false)

  // Reader and contributor columns per action (docs/utilisation/roles-et-permissions.md). A maintainer can do all.
  protected readonly roleRows: { id: string; reader: boolean; contributor: boolean }[] = [
    { id: 'read', reader: true, contributor: true },
    { id: 'write', reader: false, contributor: true },
    { id: 'merge', reader: false, contributor: false },
  ]

  protected readonly searchGroups = ['repos', 'issues', 'mrs'] as const

  protected apply(): void {
    this.applied.set(true)
  }

  protected reset(): void {
    this.applied.set(false)
  }

  protected onScroll(): void {
    const track = this.track().nativeElement
    const slides = Array.from(track.children) as HTMLElement[]
    if (slides.length === 0) {
      return
    }
    if (track.scrollLeft + track.clientWidth >= track.scrollWidth - 4) {
      this.current.set(slides.length - 1)
      return
    }
    let nearest = 0
    let distance = Infinity
    slides.forEach((slide, index) => {
      const gap = Math.abs(slide.offsetLeft - track.offsetLeft - track.scrollLeft)
      if (gap < distance) {
        distance = gap
        nearest = index
      }
    })
    this.current.set(nearest)
  }

  protected step(by: -1 | 1, event?: Event): void {
    // An arrow key is only taken over when the track itself has the focus, and for the move it makes anyway: the
    // browser would scroll by a few lines, and a control inside a tile keeps its own arrow keys.
    if (event) {
      if (event.target !== event.currentTarget) {
        return
      }
      event.preventDefault()
    }
    this.goTo(Math.min(Math.max(this.current() + by, 0), this.slides.length - 1))
  }

  protected goTo(index: number): void {
    const track = this.track().nativeElement
    const slide = track.children[index] as HTMLElement | undefined
    if (!slide) {
      return
    }
    const reduced =
      typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches
    // Scroll the track itself, not the page: `scrollIntoView` would also move the page up or down.
    track.scrollTo?.({
      left: slide.offsetLeft - track.offsetLeft,
      behavior: reduced ? 'auto' : 'smooth',
    })
    this.current.set(index)
  }
}
