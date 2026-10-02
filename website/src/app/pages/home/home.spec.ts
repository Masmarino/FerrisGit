import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { PlanHighlight } from '../../shared/plan-highlight'
import { stubMatchMedia } from '../../testing/browser-stubs'
import { BLANK_ROUTES, setUpTestApp } from '../../testing/transloco-testing'
import { Home } from './home'

describe('Home', () => {
  beforeEach(async () => {
    stubMatchMedia()
    await setUpTestApp({ imports: [Home], lang: 'en', routes: BLANK_ROUTES })
    await TestBed.inject(Router).navigateByUrl('/en')
  })

  async function render() {
    const fixture = TestBed.createComponent(Home)
    await fixture.whenStable()
    return fixture.nativeElement as HTMLElement
  }

  it('opens with one h1, the live scene and the two actions of the brief', async () => {
    const root = await render()

    expect(root.querySelectorAll('h1')).toHaveLength(1)
    expect(root.querySelector('h1')?.textContent).toBe('A Git platform in one Rust binary.')
    expect(root.querySelector('app-hero-scene')).not.toBeNull()
    const primary = root.querySelector<HTMLAnchorElement>('.hero a.gbt-button--primary')
    expect(primary?.getAttribute('href')).toBe('https://app.ferrisgit.pro')
    expect(root.querySelector('.command__text')?.textContent).toContain(
      'docker compose up -d --build',
    )
    expect(root.querySelector('gbt-copy-button')).not.toBeNull()
  })

  it('states exact figures only: one binary, one PostgreSQL, five crates, 173 routes', async () => {
    const root = await render()
    const facts = Array.from(root.querySelectorAll('.fact dt')).map((dt) => dt.textContent?.trim())
    expect(facts).toEqual(['1', '1', '5', '173', '3', '2'])
  })

  it('has the sections the header links to, in order', async () => {
    const root = await render()
    const ids = Array.from(root.querySelectorAll(':scope > section[id]')).map((s) => s.id)
    expect(ids).toEqual(['product', 'ci', 'self-hosting', 'architecture', 'security', 'roadmap'])
    expect(root.querySelector('#ci app-pipeline-sim')).not.toBeNull()
    expect(root.querySelector('#self-hosting app-install')).not.toBeNull()
    expect(root.querySelector('#architecture app-plan-figure app-architecture-plan')).not.toBeNull()
    expect(root.querySelector('.hero a[href="/en/#architecture"]')).not.toBeNull()
  })

  it('numbers seven tiles after the circles of the drawing, and lights a tile from its circle', async () => {
    const root = await render()
    for (const n of [1, 2, 3, 4, 5, 6, 7]) {
      const tile = root.querySelector(`#d${n}`)
      expect(tile, `tile ${n}`).not.toBeNull()
      expect(tile?.querySelector('.tile__no')?.textContent?.trim()).toBe(String(n))
      expect(root.querySelector(`.fig__views a[href="/en/#d${n}"]`), `link ${n}`).not.toBeNull()
    }
    const plan = TestBed.inject(PlanHighlight)
    root.querySelector('#d2')!.dispatchEvent(new MouseEvent('mouseenter'))
    expect(plan.active()).toBe(2)
    root.querySelector('#d2')!.dispatchEvent(new MouseEvent('mouseleave'))
    expect(plan.active()).toBeNull()
  })

  it('names the seven security facts and the five crates', async () => {
    const root = await render()
    expect(root.querySelectorAll('.sec__row')).toHaveLength(7)
    expect(root.textContent).toContain('Argon2')
    expect(root.textContent).toContain('AES-256-GCM')
    const crates = Array.from(root.querySelectorAll('.crates__name')).map((n) => n.textContent)
    expect(crates).toEqual([
      'ferrisgit-domain',
      'ferrisgit-application',
      'ferrisgit-infrastructure',
      'ferrisgit-api',
      'ferrisgit-runner',
    ])
  })

  it('shows the roadmap as planned, never as shipped', async () => {
    const root = await render()
    const items = root.querySelectorAll('.strip__item')
    expect(items).toHaveLength(5)
    expect(root.querySelectorAll('.strip__item .status')).toHaveLength(5)
    expect(root.querySelector('.strip__item .status')?.textContent?.trim()).toBe('Planned')
    expect(root.querySelector('a[href="/en/roadmap/"]')).not.toBeNull()
  })
})
