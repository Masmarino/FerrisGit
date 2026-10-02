import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { BLANK_ROUTES, setUpTestApp } from '../../testing/transloco-testing'
import { LanguageSwitcher } from './language-switcher'

describe('LanguageSwitcher', () => {
  beforeEach(() => setUpTestApp({ imports: [LanguageSwitcher], lang: 'fr', routes: BLANK_ROUTES }))

  async function render(url: string) {
    await TestBed.inject(Router).navigateByUrl(url)
    const fixture = TestBed.createComponent(LanguageSwitcher)
    await fixture.whenStable()
    return fixture.nativeElement as HTMLElement
  }

  it('links to the same page in each of the five languages', async () => {
    const element = await render('/fr/features')
    const links = Array.from(element.querySelectorAll('a'))

    expect(links.map((link) => link.getAttribute('href'))).toEqual([
      '/en/features/',
      '/fr/features/',
      '/it/features/',
      '/es/features/',
      '/de/features/',
    ])
    expect(links.map((link) => link.getAttribute('hreflang'))).toEqual([
      'en',
      'fr',
      'it',
      'es',
      'de',
    ])
  })

  it('names each language in itself and marks the current one', async () => {
    const element = await render('/fr/roadmap')
    const links = Array.from(element.querySelectorAll('a'))

    expect(links.map((link) => link.textContent?.trim())).toEqual([
      'English',
      'Français',
      'Italiano',
      'Español',
      'Deutsch',
    ])
    expect(links.filter((link) => link.getAttribute('aria-current') === 'true')).toHaveLength(1)
    expect(element.querySelector('[aria-current="true"]')?.textContent?.trim()).toBe('Français')
    expect(element.querySelector('summary')?.textContent).toContain('FR')
  })

  it('keeps the home page as the home page', async () => {
    const element = await render('/fr')
    const hrefs = Array.from(element.querySelectorAll('a')).map((link) => link.getAttribute('href'))
    expect(hrefs).toEqual(['/en/', '/fr/', '/it/', '/es/', '/de/'])
  })
})
