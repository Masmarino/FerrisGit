import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { JobStatus } from '@masmarino/gabarit'
import { InView } from '../motion/in-view.directive'
import { MotionService } from '../motion/motion.service'
import { Window } from '../window/window'

/** The step the prerendered page shows: the request merged. */
export const MERGED_STEP = 6

// [ms from the start of the loop, step reached]. The template reveals rows with `at(step)`: 1 push typed, 2 push
// output, 3 review comment, 4 suggestion applied, 5 approved, 6 merged, 7 pipeline page, 8 lint running, 9 test
// running, 10 passed.
const STEPS: readonly [number, number][] = [
  [400, 1],
  [1300, 2],
  [2400, 3],
  [5200, 4],
  [6600, 5],
  [8000, 6],
  [9800, 7],
  [10400, 8],
  [12000, 9],
  [13600, 10],
]
const LOOP_MS = 17000
const RESTART_FADE_MS = 500

type SceneJobState = 'pending' | 'running' | 'success'

/**
 * The hero scene: one merge request from push to merge, then the pipeline the merge starts on the default branch
 * (docs/ci-cd/premiers-pas), as the product shows them. The jobs are those of the `.ferrisgit-ci.yml` further down.
 * The prerendered page and reduced motion show the merged request, so the scene reads without scripts. The loop
 * pauses off screen (InView) and in hidden tabs (the `loop` class).
 */
@Component({
  selector: 'app-hero-scene',
  imports: [TranslocoPipe, JobStatus, Window],
  templateUrl: './hero-scene.html',
  styleUrl: './hero-scene.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  hostDirectives: [InView],
  host: {
    class: 'loop',
    '[class.is-live]': 'live()',
    '[class.is-restarting]': 'restarting()',
    '[class.is-pipeline]': 'onPipeline()',
    '[attr.data-step]': 'step()',
  },
})
export class HeroScene {
  private readonly motion = inject(MotionService)
  private timers: number[] = []

  readonly step = signal(MERGED_STEP)
  /** True once the loop runs: only then are the later steps hidden. */
  protected readonly live = signal(false)
  protected readonly restarting = signal(false)

  protected readonly merged = computed(() => this.step() >= MERGED_STEP)
  protected readonly applied = computed(() => this.step() >= 4)
  protected readonly onPipeline = computed(() => this.step() >= 7)
  protected readonly lint = computed<SceneJobState>(() => this.jobState(8, 9))
  protected readonly test = computed<SceneJobState>(() => this.jobState(9, 10))
  protected readonly url = computed(() =>
    this.onPipeline()
      ? 'git.example.com/acme/api/pipelines/a4f1c7'
      : 'git.example.com/acme/api/merge-requests/42',
  )

  constructor() {
    afterNextRender(() => {
      if (this.motion.reducedMotion()) return
      this.live.set(true)
      this.play()
    })
    inject(DestroyRef).onDestroy(() => this.clear())
  }

  protected at(n: number): boolean {
    return this.step() >= n
  }

  private jobState(runFrom: number, doneFrom: number): SceneJobState {
    const step = this.step()
    if (step >= doneFrom) return 'success'
    if (step >= runFrom) return 'running'
    return 'pending'
  }

  private play(): void {
    this.clear()
    this.restarting.set(false)
    this.step.set(0)
    for (const [ms, step] of STEPS) {
      this.timers.push(window.setTimeout(() => this.step.set(step), ms))
    }
    this.timers.push(window.setTimeout(() => this.restarting.set(true), LOOP_MS - RESTART_FADE_MS))
    this.timers.push(window.setTimeout(() => this.play(), LOOP_MS))
  }

  private clear(): void {
    for (const timer of this.timers) window.clearTimeout(timer)
    this.timers = []
  }
}
