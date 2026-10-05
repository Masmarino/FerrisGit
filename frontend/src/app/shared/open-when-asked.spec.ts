import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { openWhenAsked } from './open-when-asked';

@Component({ selector: 'fg-test-page', template: '' })
class PageWithDialog {
  opened = 0;
  constructor() {
    openWhenAsked('issue', () => this.opened++);
  }
}

describe('openWhenAsked', () => {
  async function setup(url: string) {
    TestBed.configureTestingModule({ providers: [provideRouter([{ path: 'issues', component: PageWithDialog }])] });
    const harness = await RouterTestingHarness.create();
    const page = await harness.navigateByUrl(url, PageWithDialog);
    await harness.fixture.whenStable();
    return { harness, page, router: TestBed.inject(Router) };
  }

  it('opens the dialog the URL asks for, then drops the parameter so a reload does not reopen it', async () => {
    const { page, router } = await setup('/issues?new=issue&state=open');

    expect(page.opened).toBe(1);
    expect(router.url).toBe('/issues?state=open');
  });

  it('leaves the page alone without the parameter or with another kind', async () => {
    const { page, router } = await setup('/issues?new=merge-request');

    expect(page.opened).toBe(0);
    expect(router.url).toBe('/issues?new=merge-request');
  });

  it('opens it again when asked on the page already shown', async () => {
    const { harness, page, router } = await setup('/issues');

    await harness.navigateByUrl('/issues?new=issue');
    await harness.fixture.whenStable();

    expect(page.opened).toBe(1);
    expect(router.url).toBe('/issues');
  });
});
