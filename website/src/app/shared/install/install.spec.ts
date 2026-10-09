import { TestBed } from '@angular/core/testing'
import { setUpTestApp } from '../../testing/transloco-testing'
import { Install } from './install'

describe('Install', () => {
  beforeEach(() => setUpTestApp({ imports: [Install], lang: 'en' }))

  it('renders the three ways with their real commands, the first one active', () => {
    const fixture = TestBed.createComponent(Install)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement

    const panels = root.querySelectorAll('.install__panel')
    expect(panels).toHaveLength(3)
    expect(panels[0].classList.contains('is-active')).toBe(true)
    expect(panels[0].textContent).toContain('docker compose up -d --build')
    expect(panels[1].textContent).toContain('helm upgrade --install ferrisgit ./helm/ferrisgit')
    expect(panels[2].textContent).toContain('./scripts/dev.sh')
    expect(root.querySelectorAll('gbt-copy-button')).toHaveLength(3)
  })

  it('becomes a tab set and switches with the keyboard', async () => {
    // As in a browser running the page's script: without it the tabs are hidden, and a hidden tab cannot take focus.
    const scripted = document.documentElement.classList.contains('js')
    document.documentElement.classList.add('js')
    const fixture = TestBed.createComponent(Install)
    document.body.appendChild(fixture.nativeElement)
    await fixture.whenStable()
    const root: HTMLElement = fixture.nativeElement

    const tabs = Array.from(root.querySelectorAll<HTMLElement>('[role="tab"]'))
    expect(tabs).toHaveLength(3)
    tabs[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await fixture.whenStable()
    expect(root.querySelector('#install-panel-helm')?.classList.contains('is-active')).toBe(true)
    expect(document.activeElement?.id).toBe('install-tab-helm')
    fixture.nativeElement.remove()
    document.documentElement.classList.toggle('js', scripted)
  })

  it('tells what happens at first sign-in: mandatory MFA', () => {
    const fixture = TestBed.createComponent(Install)
    fixture.detectChanges()
    const root: HTMLElement = fixture.nativeElement
    expect(root.querySelectorAll('.install__steps li')).toHaveLength(3)
    expect(root.textContent).toContain('backup codes')
  })
})
