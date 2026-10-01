import { Injectable, signal } from '@angular/core';

const COLLAPSED_KEY = 'ferrisgit_sidebar_collapsed';

@Injectable({ providedIn: 'root' })
export class SidebarCollapseService {
  readonly collapsed = signal<boolean>(localStorage.getItem(COLLAPSED_KEY) === 'true');

  set(value: boolean): void {
    localStorage.setItem(COLLAPSED_KEY, String(value));
    this.collapsed.set(value);
  }
}
