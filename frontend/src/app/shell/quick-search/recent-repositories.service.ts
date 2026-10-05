import { Injectable, signal } from '@angular/core';

const MAX_RECENT = 5;

/**
 * The repositories an account last opened in this browser, newest first, offered by the quick search before anything is
 * typed. Kept per account so a shared browser doesn't show one person's repositories to the next. Storage can be
 * missing or full (private windows): the list then lives as long as the page.
 */
@Injectable({ providedIn: 'root' })
export class RecentRepositoriesService {
  // Bumped on every change, so a computed that reads `list()` follows writes made through `remember()`.
  private readonly version = signal(0);
  private readonly fallback = new Map<string, string[][]>();

  list(userId: string): string[][] {
    this.version();
    if (!userId) {
      return [];
    }
    return this.read(userId);
  }

  remember(userId: string, path: string[]): void {
    if (!userId || path.length === 0) {
      return;
    }
    const key = path.join('/');
    const next = [path, ...this.read(userId).filter((p) => p.join('/') !== key)].slice(0, MAX_RECENT);
    this.fallback.set(userId, next);
    try {
      localStorage.setItem(storageKey(userId), JSON.stringify(next));
    } catch {
      // Storage unavailable: the in-memory copy above is enough for this page.
    }
    this.version.update((v) => v + 1);
  }

  private read(userId: string): string[][] {
    try {
      const raw = localStorage.getItem(storageKey(userId));
      if (raw !== null) {
        const parsed: unknown = JSON.parse(raw);
        if (Array.isArray(parsed)) {
          return parsed.filter(isPath).slice(0, MAX_RECENT);
        }
      }
    } catch {
      // Unreadable or corrupt: fall back to what this page remembered.
    }
    return this.fallback.get(userId) ?? [];
  }
}

function storageKey(userId: string): string {
  return `fg.recent-repositories.${userId}`;
}

function isPath(value: unknown): value is string[] {
  return Array.isArray(value) && value.length > 0 && value.every((segment) => typeof segment === 'string' && segment !== '');
}
