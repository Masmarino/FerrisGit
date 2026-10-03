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

  it('opens with one h1 that says what it solves, and the two actions of the brief', async () => {
    const root = await render()

    expect(root.querySelectorAll('h1')).toHaveLength(1)
    expect(root.querySelector('h1')?.textContent).toBe(
      'A lightweight Git forge that runs on the infrastructure you already have.',
    )
    const primary = root.querySelector<HTMLAnchorElement>('.hero a.gbt-button--primary')
    expect(primary?.getAttribute('href')).toBe('https://app.ferrisgit.pro')
    expect(root.querySelector('.command__text')?.textContent).toContain(
      'docker compose up -d --build',
    )
    expect(root.querySelector('gbt-copy-button')).not.toBeNull()
  })

  it('backs the claim with the output of the command that measures it, and says how', async () => {
    const root = await render()

    const terminal = root.querySelector('.proof__terminal')
    expect(terminal?.textContent).toContain('docker stats')
    expect(terminal?.textContent).toMatch(/\d+(\.\d+)?MiB/)
    expect(root.querySelector('.proof figcaption')?.textContent).toContain('version')
    expect(root.querySelector('.proof a')?.getAttribute('href')).toBe(
      'https://app.ferrisgit.pro/docs/administration/ressources',
    )
  })

  it('answers four situations, each with the need and what changes', async () => {
    const root = await render()

    const needs = Array.from(root.querySelectorAll('#why .why__row dt')).map((dt) =>
      dt.textContent?.trim(),
    )
    expect(needs).toHaveLength(4)
    expect(needs[0]).toContain('dedicate a machine to the forge')
    expect(root.querySelectorAll('#why .why__row dd')).toHaveLength(4)
    expect(root.querySelector('#why')?.textContent).toContain('a test checks it')
  })

  it('says what is missing before it shows the roadmap', async () => {
    const root = await render()

    const missing = Array.from(root.querySelectorAll('#roadmap .limits__item dt')).map((dt) =>
      dt.textContent?.trim(),
    )
    expect(missing).toEqual([
      'No SSH',
      'No protected branches',
      'The interface is in French only',
      'No code analysis',
    ])
    const limits = root.querySelector('#roadmap .limits')!
    const strip = root.querySelector('#roadmap .strip')!
    expect(limits.compareDocumentPosition(strip) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('has the four sections of the overview, in order, and leaves the detail to the other pages', async () => {
    const root = await render()
    const ids = Array.from(root.querySelectorAll(':scope > section[id]')).map((s) => s.id)
    expect(ids).toEqual(['why', 'product', 'architecture', 'roadmap'])
    expect(root.querySelector('#architecture app-plan-figure app-architecture-plan')).not.toBeNull()
    expect(root.querySelector('#product app-capabilities')).not.toBeNull()
    expect(root.querySelector('.hero a[href="/en/#architecture"]')).not.toBeNull()
    // The pipeline demo, the install tabs and the security list each have a page of their own.
    expect(root.querySelector('app-pipeline-sim, app-install, .sec__row')).toBeNull()
    expect(root.querySelector('#product a[href="/en/features/"]')).not.toBeNull()
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

  it('names the five crates', async () => {
    const root = await render()
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
