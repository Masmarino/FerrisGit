import { DOCUMENT, DestroyRef, Injectable, Signal, inject, signal } from '@angular/core';
import { CanDeactivateFn } from '@angular/router';

/** Asked before leaving: true to leave, false to stay. May ask the person first. */
export type LeaveCheck = () => boolean | Promise<boolean>;

/**
 * Work that leaving the page would lose. A page that holds some registers a check while it is shown; the router asks it
 * before going anywhere else (see `pendingChangesGuard`). One page at a time: the latest registration wins.
 */
@Injectable({ providedIn: 'root' })
export class PendingChanges {
  private check: LeaveCheck | null = null;

  /** Returns what to call when the page goes, so that it stops being asked. */
  register(check: LeaveCheck): () => void {
    this.check = check;
    return () => {
      if (this.check === check) {
        this.check = null;
      }
    };
  }

  canLeave(): boolean | Promise<boolean> {
    return this.check ? this.check() : true;
  }
}

/**
 * For a route whose pages can hold unsaved work. A route reused between its pages (`repositories/**`) asks on every move
 * between them too: its URL segments change, and that reruns its guards.
 */
export const pendingChangesGuard: CanDeactivateFn<unknown> = () => inject(PendingChanges).canLeave();

/**
 * For a page that holds work while `holdsWork()` says so: leaving it for another page asks first, through the page's own
 * dialog (open while `asking()`, closed by `answer`), and closing the tab or reloading asks through the browser's.
 * Call it in an injection context; it lets go when the page goes.
 */
export function confirmLeaving(holdsWork: () => boolean): { asking: Signal<boolean>; answer: (leave: boolean) => void } {
  const asking = signal(false);
  let pending: ((leave: boolean) => void) | null = null;
  const answer = (leave: boolean) => {
    asking.set(false);
    const resolve = pending;
    pending = null;
    resolve?.(leave);
  };
  const unregister = inject(PendingChanges).register(() => {
    if (!holdsWork()) {
      return true;
    }
    answer(false);
    asking.set(true);
    return new Promise<boolean>((resolve) => (pending = resolve));
  });
  // Only the browser's own question can stop a tab from closing.
  const onBeforeUnload = (event: BeforeUnloadEvent) => {
    if (holdsWork()) {
      event.preventDefault();
      // Safari and older browsers still read this rather than the call above.
      event.returnValue = '';
    }
  };
  const window = inject(DOCUMENT).defaultView;
  window?.addEventListener('beforeunload', onBeforeUnload);
  inject(DestroyRef).onDestroy(() => {
    unregister();
    window?.removeEventListener('beforeunload', onBeforeUnload);
    answer(false);
  });
  return { asking: asking.asReadonly(), answer };
}
