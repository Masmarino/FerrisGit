import { DestroyRef, Directive, ElementRef, afterNextRender, inject, input } from '@angular/core'
import { MotionService } from './motion.service'

/**
 * Follows the pointer a little: sets --px and --py on the host, from -amplitude/2 to +amplitude/2 pixels, for the
 * stylesheet to turn into transforms. Only for devices that hover, and not with reduced motion.
 */
@Directive({ selector: '[appParallax]' })
export class Parallax {
  /** Pixels between the two extremes of the offset. */
  readonly amplitude = input(14, { alias: 'appParallax' })

  constructor() {
    const element: HTMLElement = inject(ElementRef).nativeElement
    const motion = inject(MotionService)
    const destroyRef = inject(DestroyRef)

    afterNextRender(() => {
      if (motion.reducedMotion() || !window.matchMedia('(hover: hover)').matches) return
      let frame = 0
      let x = 0
      let y = 0
      const paint = () => {
        frame = 0
        element.style.setProperty('--px', `${x}px`)
        element.style.setProperty('--py', `${y}px`)
      }
      const schedule = () => {
        if (!frame) frame = requestAnimationFrame(paint)
      }
      const onMove = (event: PointerEvent) => {
        const box = element.getBoundingClientRect()
        x = ((event.clientX - box.left) / box.width - 0.5) * this.amplitude()
        y = ((event.clientY - box.top) / box.height - 0.5) * this.amplitude()
        schedule()
      }
      const onLeave = () => {
        x = 0
        y = 0
        schedule()
      }
      element.addEventListener('pointermove', onMove, { passive: true })
      element.addEventListener('pointerleave', onLeave)
      destroyRef.onDestroy(() => {
        element.removeEventListener('pointermove', onMove)
        element.removeEventListener('pointerleave', onLeave)
        cancelAnimationFrame(frame)
      })
    })
  }
}
