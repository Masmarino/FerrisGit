import { Injectable, inject } from '@angular/core';
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
