import { buildSitemap, pageUrls } from '../../../scripts/sitemap.mjs'
import site from './site.json'
import { LANGS, PAGE_IDS } from '../i18n/languages'
import { pageUrl } from './page-meta'

describe('sitemap', () => {
  it('lists the 15 pages: 3 pages in 5 languages', () => {
    expect(pageUrls(site)).toHaveLength(LANGS.length * PAGE_IDS.length)
    expect(pageUrls(site)).toHaveLength(15)
  })

  it('uses the same URLs as the canonical links of the pages', () => {
    const expected = PAGE_IDS.flatMap((page) => LANGS.map((lang) => pageUrl(lang, page))).sort()
    expect([...pageUrls(site)].sort()).toEqual(expected)
  })

  it('declares every language and x-default on each entry', () => {
    const xml = buildSitemap(site)
    expect((xml.match(/<url>/g) ?? []).length).toBe(15)
    // 15 entries x (5 languages + x-default)
    expect((xml.match(/<xhtml:link /g) ?? []).length).toBe(15 * 6)
    expect(xml).toContain('hreflang="x-default" href="https://www.ferrisgit.pro/en/roadmap/"')
  })
})
