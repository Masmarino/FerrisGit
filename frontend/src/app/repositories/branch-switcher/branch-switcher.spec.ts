import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { BranchSwitcher } from './branch-switcher';
import { provideFerrisgitIcons } from '../../shared/register-icons';

@Component({ template: '' })
class DummyRoutedComponent {}

describe('BranchSwitcher', () => {
  function setup() {
    // Without a wildcard route, router.navigate() rejects.
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: DummyRoutedComponent }]), provideFerrisgitIcons()],
    });
    const fixture = TestBed.createComponent(BranchSwitcher);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    fixture.componentRef.setInput('currentRef', 'main');
    const http = TestBed.inject(HttpTestingController);
    return { fixture, http, router: TestBed.inject(Router) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists branches and tags as select options', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();

    http.expectOne('/api/repositories/repo-1/branches').flush([{ name: 'main', tipSha: 'a', isDefault: true }, { name: 'feature', tipSha: 'b', isDefault: false }]);
    http.expectOne('/api/repositories/repo-1/tags').flush([{ name: 'v1.0.0', targetSha: 'a' }]);
    fixture.detectChanges();

    const select: HTMLElement = fixture.nativeElement.querySelector('gbt-select');
    expect(select).toBeTruthy();
  });

  it('is a compact select whose accessible name says it picks a branch or a tag', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/repo-1/branches').flush([{ name: 'main', tipSha: 'a', isDefault: true }]);
    http.expectOne('/api/repositories/repo-1/tags').flush([]);
    fixture.detectChanges();

    const select: HTMLElement = fixture.nativeElement.querySelector('gbt-select');
    expect(select.classList).toContain('gbt-select--sm');
    const trigger = select.querySelector('[role="combobox"]') as HTMLElement;
    const labelledBy = (trigger.getAttribute('aria-labelledby') ?? '').split(' ');
    const label = labelledBy.map((id) => fixture.nativeElement.querySelector(`#${id}`)).find((el: Element | null) => el?.tagName === 'LABEL');
    expect(label?.textContent?.trim()).toBe('Branche ou tag');
    expect(label?.classList).toContain('gbt-select__label--hidden');
  });

  it('marks branches and tags with their own icon, shown on the trigger for the current ref', async () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/repo-1/branches').flush([{ name: 'main', tipSha: 'a', isDefault: true }]);
    http.expectOne('/api/repositories/repo-1/tags').flush([{ name: 'v1.0.0', targetSha: 'a' }]);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();

    const trigger = fixture.nativeElement.querySelector('[role="combobox"]') as HTMLElement;
    expect(trigger.querySelector('.gbt-select__trigger-icon svg')).toBeTruthy();
    trigger.click();
    fixture.detectChanges();
    const options: HTMLElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="option"]'));
    expect(options.map((o) => o.textContent?.trim())).toEqual(['main', 'v1.0.0']);
    const icons = options.map((o) => (o.querySelector('.gbt-select__option-icon svg') as SVGElement | null)?.innerHTML ?? '');
    expect(icons[0]).not.toBe('');
    expect(icons[0]).not.toBe(icons[1]);
  });

  it('navigates to the tree route for the chosen ref when a new option is picked', async () => {
    const { fixture, http, router } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/repo-1/branches').flush([{ name: 'main', tipSha: 'a', isDefault: true }, { name: 'feature', tipSha: 'b', isDefault: false }]);
    http.expectOne('/api/repositories/repo-1/tags').flush([]);
    fixture.detectChanges();

    await fixture.componentInstance.navigateToRef('feature');

    expect(router.url).toBe('/repositories/alice/hello/-/tree/feature');
  });
});
