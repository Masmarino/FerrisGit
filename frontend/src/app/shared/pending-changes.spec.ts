import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { PendingChanges, pendingChangesGuard } from './pending-changes';

@Component({ template: '' })
class Page {}

describe('PendingChanges', () => {
  const service = () => TestBed.inject(PendingChanges);

  it('lets one leave when no page holds unsaved work', () => {
    expect(service().canLeave()).toBe(true);
  });

  it('asks the page that registered, until it unregisters', async () => {
    const unregister = service().register(() => Promise.resolve(false));

    await expect(service().canLeave()).resolves.toBe(false);

    unregister();
    expect(service().canLeave()).toBe(true);
  });

  it('keeps the latest page when an older one unregisters after it', () => {
    const older = service().register(() => false);
    service().register(() => false);

    older();

    expect(service().canLeave()).toBe(false);
  });

  describe('on a route shared by many pages', () => {
    async function onEditor() {
      TestBed.configureTestingModule({
        providers: [
          provideRouter([
            { path: 'repositories/**', canDeactivate: [pendingChangesGuard], component: Page },
            { path: 'home', component: Page },
          ]),
        ],
      });
      // A reused route's guard runs only when an outlet shows its page, as the shell's does.
      await RouterTestingHarness.create('/repositories/acme/widget/-/pipelines/editor');
      return TestBed.inject(Router);
    }

    it('asks before moving to another page of the same route', async () => {
      const router = await onEditor();
      service().register(() => false);

      expect(await router.navigateByUrl('/repositories/acme/widget/-/pipelines')).toBe(false);
      expect(router.url).toBe('/repositories/acme/widget/-/pipelines/editor');
    });

    it('asks before leaving the route, and goes once allowed', async () => {
      const router = await onEditor();
      let answer = false;
      service().register(() => answer);

      expect(await router.navigateByUrl('/home')).toBe(false);
      answer = true;
      expect(await router.navigateByUrl('/home')).toBe(true);
    });
  });
});
