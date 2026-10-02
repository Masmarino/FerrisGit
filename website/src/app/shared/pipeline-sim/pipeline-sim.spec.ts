import { TestBed } from '@angular/core/testing'
import { FakeIntersectionObserver, stubMatchMedia } from '../../testing/browser-stubs'
import { setUpTestApp } from '../../testing/transloco-testing'
import { MotionService } from '../motion/motion.service'
import { PipelineSim } from './pipeline-sim'

describe('PipelineSim', () => {
  beforeEach(() => setUpTestApp({ imports: [PipelineSim], lang: 'en' }))

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    FakeIntersectionObserver.reset()
  })

  function states(root: HTMLElement): (string | null)[] {
    return Array.from(root.querySelectorAll('.sim__job')).map((job) =>
      job.getAttribute('data-state'),
    )
  }

  it('shows the example file beside its finished pipeline before any script runs', () => {
    const fixture = TestBed.createComponent(PipelineSim)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    expect(root.textContent).toContain('needs: [format, clippy]')
    expect(states(root)).toEqual(['success', 'success', 'success'])
    expect(root.querySelector('.sim__status')?.textContent).toContain('Pipeline passed')
    expect(root.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('false')
  })

  it('runs the jobs in the order of the scheduler once it scrolls into view', async () => {
    vi.useFakeTimers()
    stubMatchMedia({ reducedMotion: false })
    vi.stubGlobal('IntersectionObserver', FakeIntersectionObserver)
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(PipelineSim)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    FakeIntersectionObserver.instances[0].trigger(root, true)
    vi.advanceTimersByTime(10)
    fixture.detectChanges()
    expect(states(root)).toEqual(['pending', 'pending', 'pending'])

    vi.advanceTimersByTime(400)
    fixture.detectChanges()
    expect(states(root)).toEqual(['running', 'running', 'pending'])

    vi.advanceTimersByTime(2400)
    fixture.detectChanges()
    expect(states(root)).toEqual(['success', 'success', 'running'])

    vi.advanceTimersByTime(1600)
    fixture.detectChanges()
    expect(states(root)).toEqual(['success', 'success', 'success'])
  })

  it('skips test, without running it, when clippy fails: the rule of needs', async () => {
    vi.useFakeTimers()
    stubMatchMedia({ reducedMotion: false })
    vi.stubGlobal('IntersectionObserver', FakeIntersectionObserver)
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(PipelineSim)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    root.querySelector<HTMLButtonElement>('[role="switch"]')!.click()
    fixture.detectChanges()
    expect(root.querySelector('[role="switch"]')?.getAttribute('aria-checked')).toBe('true')
    expect(root.querySelector('.code__line.is-marked')?.textContent).toContain('-D warnings')

    vi.advanceTimersByTime(2800)
    fixture.detectChanges()
    expect(states(root)).toEqual(['success', 'failed', 'skipped'])
    expect(root.querySelector('.sim__status')?.getAttribute('data-state')).toBe('failed')
    expect(root.querySelector('.sim__note')?.textContent).toContain('skipped')
    // A skipped job shows the product's cancelled glyph.
    expect(
      root
        .querySelector('.sim__job[data-state="skipped"] gbt-job-status')
        ?.getAttribute('data-status'),
    ).toBe('canceled')
  })

  it('jumps to the end of the run under reduced motion', async () => {
    stubMatchMedia({ reducedMotion: true })
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(PipelineSim)
    await fixture.whenStable()
    const root: HTMLElement = fixture.nativeElement

    root.querySelector<HTMLButtonElement>('[role="switch"]')!.click()
    await fixture.whenStable()
    expect(states(root)).toEqual(['success', 'failed', 'skipped'])
  })
})
