import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { stubMatchMedia } from '../../testing/browser-stubs'
import { BLANK_ROUTES, setUpTestApp } from '../../testing/transloco-testing'
import { Ci } from './ci'

describe('Ci', () => {
  beforeEach(async () => {
    stubMatchMedia()
    await setUpTestApp({ imports: [Ci], lang: 'en', routes: BLANK_ROUTES })
    await TestBed.inject(Router).navigateByUrl('/en/ci')
  })

  async function render() {
    const fixture = TestBed.createComponent(Ci)
    await fixture.whenStable()
    return fixture.nativeElement as HTMLElement
  }

  it('has one h1, and a path that says where the page sits', async () => {
    const root = await render()

    expect(root.querySelectorAll('h1')).toHaveLength(1)
    expect(root.querySelector('h1')?.textContent).toBe('From push to pipeline')
    expect(root.querySelector('.crumb')?.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'FerrisGit / CI/CD',
    )
    expect(root.querySelector('.crumb a')?.getAttribute('href')).toBe('/en/')
  })

  it('runs the pipeline demo, then says how jobs run, what an error looks like and what variables are', async () => {
    const root = await render()

    expect(root.querySelector('app-pipeline-sim')).not.toBeNull()
    const terms = Array.from(root.querySelectorAll('.spec__row dt')).map((dt) =>
      dt.textContent?.trim(),
    )
    expect(terms).toEqual(['Two ways to run a job', 'Explicit errors', 'Variables and caches'])
    expect(
      root.querySelector('a[href="https://app.ferrisgit.pro/docs/ci-cd/reference-yaml"]'),
    ).not.toBeNull()
  })

  it('ends on the call to try or install', async () => {
    const root = await render()

    expect(root.querySelector('app-cta .gbt-button--primary')?.getAttribute('href')).toBe(
      'https://app.ferrisgit.pro',
    )
  })
})
