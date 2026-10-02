import { Component } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { stubMatchMedia } from '../../testing/browser-stubs'
import { MotionService } from './motion.service'
import { Parallax } from './parallax.directive'

@Component({ imports: [Parallax], template: '<div id="area" [appParallax]="20"></div>' })
class Host {}

describe('Parallax', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    document.documentElement.className = ''
  })

  async function render(options: { hover?: boolean; reducedMotion?: boolean }) {
    // Frames run when the test says so.
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) =>
      frames.push(callback),
    )
    stubMatchMedia(options)
    TestBed.inject(MotionService).init()
    const fixture = TestBed.createComponent(Host)
    await fixture.whenStable()
    const area: HTMLElement = fixture.nativeElement.querySelector('#area')
    area.getBoundingClientRect = () => ({ left: 100, top: 50, width: 200, height: 100 }) as DOMRect
    const flush = () => frames.splice(0).forEach((callback) => callback(0))
    const move = (clientX: number, clientY: number) => {
      area.dispatchEvent(new MouseEvent('pointermove', { clientX, clientY }))
      flush()
    }
    return { area, move, flush }
  }

  it('moves the offset with the pointer, by half the amplitude at most, and resets on leave', async () => {
    const { area, move, flush } = await render({ hover: true })

    move(300, 150)
    expect(area.style.getPropertyValue('--px')).toBe('10px')
    expect(area.style.getPropertyValue('--py')).toBe('10px')
    move(200, 100)
    expect(area.style.getPropertyValue('--px')).toBe('0px')

    move(100, 50)
    expect(area.style.getPropertyValue('--px')).toBe('-10px')
    area.dispatchEvent(new MouseEvent('pointerleave'))
    flush()
    expect(area.style.getPropertyValue('--px')).toBe('0px')
    expect(area.style.getPropertyValue('--py')).toBe('0px')
  })

  it('leaves the host alone without a hovering pointer or under reduced motion', async () => {
    for (const options of [{ hover: false }, { hover: true, reducedMotion: true }]) {
      const { area, move } = await render(options)
      move(300, 150)
      expect(area.style.getPropertyValue('--px')).toBe('')
      TestBed.resetTestingModule()
    }
  })
})
