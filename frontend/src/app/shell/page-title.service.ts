import { Injectable, signal } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class PageTitleService {
  readonly title = signal('FerrisGit');

  set(title: string): void {
    this.title.set(title);
  }
}
