import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  inject,
  signal,
} from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { JobStatus } from '@masmarino/gabarit'
import { CodeBlock } from '../code-block/code-block'
import { MotionService } from '../motion/motion.service'
import { ViewportObserver } from '../motion/viewport-observer'
import { CI_EXAMPLE } from '../snippets'

type JobState = 'pending' | 'running' | 'success' | 'failed' | 'skipped'
type PipelineState = 'created' | 'running' | 'success' | 'failed'

interface Snapshot {
  format: JobState
  clippy: JobState
  test: JobState
  pipeline: PipelineState
}

const DONE: Snapshot = {
  format: 'success',
  clippy: 'success',
  test: 'success',
  pipeline: 'success',
}

// What the scheduler does with the example file, as [ms from the start of a run, state]. format and clippy start
// together; test needs both, so it waits for them and is skipped without running as soon as one fails
// (docs/ci-cd/reference-yaml.md).
function timeline(clippyFails: boolean): [number, Snapshot][] {
  const steps: [number, Snapshot][] = [
    [0, { format: 'pending', clippy: 'pending', test: 'pending', pipeline: 'created' }],
    [350, { format: 'running', clippy: 'running', test: 'pending', pipeline: 'running' }],
    [1600, { format: 'success', clippy: 'running', test: 'pending', pipeline: 'running' }],
  ]
  if (clippyFails) {
    steps.push([2700, { format: 'success', clippy: 'failed', test: 'skipped', pipeline: 'failed' }])
  } else {
    steps.push([
      2700,
      { format: 'success', clippy: 'success', test: 'running', pipeline: 'running' },
    ])
    steps.push([4300, DONE])
  }
  return steps
}

// The line that makes clippy fail when the switch is on: -D warnings turns every lint into an error.
const CLIPPY_LINE = 16

/**
 * The CI/CD demo: the example `.ferrisgit-ci.yml` next to the pipeline it produces. It plays when it scrolls into
 * view, and the switch replays it with a failing clippy to show what `needs` does. The prerendered page and reduced
 * motion show the finished, successful run.
 */
@Component({
  selector: 'app-pipeline-sim',
  imports: [TranslocoPipe, JobStatus, CodeBlock],
  templateUrl: './pipeline-sim.html',
  styleUrl: './pipeline-sim.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PipelineSim {
  private readonly motion = inject(MotionService)
  private readonly viewport = inject(ViewportObserver)
  private readonly element: HTMLElement = inject(ElementRef).nativeElement
  private timers: number[] = []

  protected readonly code = CI_EXAMPLE
  protected readonly failing = signal(false)
  protected readonly state = signal<Snapshot>(DONE)
  protected readonly pipelineStatus = computed(() => {
    const { pipeline } = this.state()
    return pipeline === 'success' || pipeline === 'failed' ? pipeline : 'running'
  })
  protected readonly markedLines = computed(() => (this.failing() ? [CLIPPY_LINE] : []))
  protected readonly jobs = computed(() => {
    const s = this.state()
    return {
      lint: [
        { name: 'format', state: s.format },
        { name: 'clippy', state: s.clippy },
      ],
      test: [{ name: 'test', state: s.test }],
    }
  })

  constructor() {
    afterNextRender(() => {
      if (this.motion.reducedMotion() || !this.viewport.supported) return
      this.viewport.observeOnce(this.element, () => this.play())
    })
    inject(DestroyRef).onDestroy(() => {
      this.clear()
      this.viewport.unobserve(this.element)
    })
  }

  /** The glyph of the product for a state: a skipped job shows the cancelled mark, as the application does. */
  protected glyph(state: JobState): 'pending' | 'running' | 'success' | 'failed' | 'canceled' {
    return state === 'skipped' ? 'canceled' : state
  }

  protected toggle(): void {
    this.failing.update((value) => !value)
    this.play()
  }

  protected play(): void {
    this.clear()
    const steps = timeline(this.failing())
    if (this.motion.reducedMotion()) {
      this.state.set(steps[steps.length - 1][1])
      return
    }
    for (const [ms, snapshot] of steps) {
      this.timers.push(window.setTimeout(() => this.state.set(snapshot), ms))
    }
  }

  private clear(): void {
    for (const timer of this.timers) window.clearTimeout(timer)
    this.timers = []
  }
}
