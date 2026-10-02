import { TestBed } from '@angular/core/testing'
import { stubMatchMedia } from '../../testing/browser-stubs'
import { setUpTestApp } from '../../testing/transloco-testing'
import { MotionService } from '../motion/motion.service'
import { HeroScene, MERGED_STEP } from './hero-scene'

describe('HeroScene', () => {
  beforeEach(() => setUpTestApp({ imports: [HeroScene], lang: 'en' }))

  afterEach(() => {
    vi.useRealTimers()
    document.documentElement.className = ''
  })

  it('renders the merged request before the loop starts, and describes the scene in text', () => {
    stubMatchMedia({ reducedMotion: true })
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(HeroScene)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    expect(fixture.componentInstance.step()).toBe(MERGED_STEP)
    expect(root.querySelector('.scene__badge')?.textContent?.trim()).toBe('Merged')
    expect(root.querySelector('.scene__applied')?.textContent).toContain('b7e2c91')
    expect(root.querySelector('.scene')?.getAttribute('aria-hidden')).toBe('true')
    expect(root.querySelector('.sr-only')?.textContent).toContain('merged')
    expect(root.querySelector('.window__url')?.textContent).toContain('merge-requests/42')
  })

  it('plays from the push to the pipeline, then loops, unless motion is reduced', async () => {
    vi.useFakeTimers()
    stubMatchMedia({ reducedMotion: false })
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(HeroScene)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    expect(root.classList.contains('is-live')).toBe(true)
    expect(fixture.componentInstance.step()).toBe(0)
    expect(root.querySelector('.scene__button')?.textContent?.trim()).toBe('Apply suggestion')

    vi.advanceTimersByTime(8100)
    fixture.detectChanges()
    expect(fixture.componentInstance.step()).toBe(6)
    expect(root.querySelector('.scene__badge')?.textContent?.trim()).toBe('Merged')

    vi.advanceTimersByTime(6000)
    fixture.detectChanges()
    expect(root.classList.contains('is-pipeline')).toBe(true)
    expect(root.querySelector('.window__url')?.textContent).toContain('pipelines/a4f1c7')
    expect(
      Array.from(root.querySelectorAll('.scene__page--pipeline .scene__job')).map((job) =>
        job.getAttribute('data-state'),
      ),
    ).toEqual(['success', 'success', 'success'])

    vi.advanceTimersByTime(2950)
    fixture.detectChanges()
    expect(fixture.componentInstance.step()).toBe(0)
  })

  it('stays on the finished request under reduced motion', async () => {
    stubMatchMedia({ reducedMotion: true })
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(HeroScene)
    await fixture.whenStable()
    expect(fixture.nativeElement.classList.contains('is-live')).toBe(false)
    expect(fixture.componentInstance.step()).toBe(MERGED_STEP)
  })
})
