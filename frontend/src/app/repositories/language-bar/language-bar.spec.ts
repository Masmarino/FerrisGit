import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { LanguageBar, colorForLanguage } from './language-bar';

describe('colorForLanguage', () => {
  it('assigns pairwise distinct colors to languages that used to collide via hashing', () => {
    const rust = colorForLanguage('Rust');
    const html = colorForLanguage('HTML');
    const css = colorForLanguage('CSS');

    expect(rust).not.toBe(html);
    expect(rust).not.toBe(css);
    expect(html).not.toBe(css);
  });

  it('uses the fixed color of a known language', () => {
    expect(colorForLanguage('Rust')).toBe('#dea584');
    expect(colorForLanguage('Other')).toBe('#8b8b8b');
  });

  it('falls back to a stable palette color for a name outside the known set', () => {
    const first = colorForLanguage('Brainfuck');

    expect(first).toMatch(/^#[0-9a-f]{6}$/);
    expect(colorForLanguage('Brainfuck')).toBe(first);
    expect(['#dea584', '#f1e05a', '#3572a5', '#e34c26', '#563d7c', '#00add8', '#701516']).toContain(first);
  });

  it('gives an empty name a palette color instead of failing', () => {
    expect(colorForLanguage('')).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe('LanguageBar', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const fixture = TestBed.createComponent(LanguageBar);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('ref', 'main');
    return { fixture, http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('renders a segment and a legend entry per language', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [{ name: 'Rust', bytes: 75, percentage: 75 }, { name: 'HTML', bytes: 25, percentage: 25 }] });
    fixture.detectChanges();

    const segments = fixture.nativeElement.querySelectorAll('.language-bar__segment');
    expect(segments.length).toBe(2);
    expect((segments[0] as HTMLElement).style.width).toBe('75%');
    expect(fixture.nativeElement.textContent).toContain('Rust');
    expect(fixture.nativeElement.textContent).toContain('HTML');
  });

  it('names each language and its share in the legend, with a one-decimal French percentage', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [{ name: 'Rust', bytes: 62, percentage: 62.44 }, { name: 'CSS', bytes: 38, percentage: 37.56 }] });
    fixture.detectChanges();

    const items: HTMLElement[] = Array.from(fixture.nativeElement.querySelectorAll('.language-bar__legend li'));
    expect(items.map((li) => li.querySelector('.language-bar__name')?.textContent?.trim())).toEqual(['Rust', 'CSS']);
    expect(items.map((li) => li.querySelector('.language-bar__percent')?.textContent?.trim())).toEqual(['62,4 %', '37,6 %']);
  });

  it('gives the bar an accessible summary, the segments being decorative', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [{ name: 'Rust', bytes: 75, percentage: 75 }, { name: 'HTML', bytes: 25, percentage: 25 }] });
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.language-bar__segments').getAttribute('aria-hidden')).toBe('true');
  });

  it('shows a skeleton while the languages load', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeTruthy();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [] });
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeNull();
  });

  it('renders no bar but a short muted message when there are no languages', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [] });
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.language-bar')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Aucun langage détecté');
  });

  it('treats a failed request like no languages instead of loading forever', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush('boom', { status: 500, statusText: 'Internal Server Error' });
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Aucun langage détecté');
  });
});
