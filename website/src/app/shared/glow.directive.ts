import { Directive, ElementRef, inject } from '@angular/core'

/**
 * Follows the pointer over a tile by setting --mx/--my on the host; styles.scss (.tile) turns them into a ring and a
 * glow. Does nothing without a pointer.
 */
@Directive({
  selector: '[appGlow]',
  host: { '(pointermove)': 'move($event)' },
})
export class Glow {
  private readonly element: HTMLElement = inject(ElementRef).nativeElement

  protected move(event: PointerEvent): void {
    const box = this.element.getBoundingClientRect()
    this.element.style.setProperty('--mx', `${event.clientX - box.left}px`)
    this.element.style.setProperty('--my', `${event.clientY - box.top}px`)
  }
}
