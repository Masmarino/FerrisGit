import { TestBed } from '@angular/core/testing'
import { setUpTestApp } from '../../testing/transloco-testing'
import { Capabilities } from './capabilities'

describe('Capabilities', () => {
  async function render(lang: 'en' | 'fr' = 'en') {
    await setUpTestApp({ imports: [Capabilities], lang })
    const fixture = TestBed.createComponent(Capabilities)
    await fixture.whenStable()
    return fixture
  }

  it('shows eight tiles, each with a heading and a fragment of interface', async () => {
    const root: HTMLElement = (await render()).nativeElement
    const tiles = root.querySelectorAll('.tile')
    expect(tiles).toHaveLength(8)
    for (const tile of Array.from(tiles)) {
      expect(tile.querySelector('h3')?.textContent?.trim().length).toBeGreaterThan(0)
      expect(tile.querySelector('.frag')).not.toBeNull()
    }
  })

  it('applies the suggestion of the review tile and lets it be undone', async () => {
    const fixture = await render('fr')
    const root: HTMLElement = fixture.nativeElement

    const apply = root.querySelector<HTMLButtonElement>('.frag__button')!
    expect(apply.textContent?.trim()).toBe('Appliquer la suggestion')
    apply.click()
    await fixture.whenStable()
    expect(root.querySelector('.applied')?.textContent).toContain('b7e2c91')
    expect(root.querySelector('.frag--review')?.classList.contains('is-applied')).toBe(true)

    root.querySelector<HTMLButtonElement>('.frag__reset')!.click()
    await fixture.whenStable()
    expect(root.querySelector('.frag__button')).not.toBeNull()
  })

  it('states the facts of the documentation: the signature header, the roles, the four search groups', async () => {
    const root: HTMLElement = (await render()).nativeElement

    expect(root.textContent).toContain('X-FerrisGit-Signature-256')
    expect(root.textContent).toContain('merge_request_merged')
    const marks = Array.from(root.querySelectorAll('.roles tbody tr')).map((row) =>
      Array.from(row.querySelectorAll('.roles__mark')).map((m) => m.classList.contains('is-on')),
    )
    expect(marks).toEqual([
      [true, true, true],
      [false, true, true],
      [false, false, true],
    ])
    expect(root.querySelectorAll('.search__results li')).toHaveLength(3)
    expect(root.querySelectorAll('.factor')).toHaveLength(3)
  })
})
