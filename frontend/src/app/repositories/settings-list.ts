import { inject, signal } from '@angular/core';
import { GbtToastService } from '@masmarino/gabarit';
import { Observable } from 'rxjs';

/**
 * The list a settings section shows. The state is 'loading' until the first response; if a later
 * refresh fails the previous list stays on screen and only a toast reports it. Call in an injection context.
 */
export function createSettingsList<T>(fetch: () => Observable<T[]>) {
  const toast = inject(GbtToastService);
  const items = signal<T[]>([]);
  const state = signal<'loading' | 'loaded' | 'failed'>('loading');

  function refresh(): void {
    fetch().subscribe({
      next: (list) => {
        items.set(list);
        state.set('loaded');
      },
      error: () => {
        if (state() !== 'loaded') {
          state.set('failed');
        }
        toast.show('Impossible de charger les réglages. Réessayez plus tard.', 'error');
      },
    });
  }

  function retry(): void {
    state.set('loading');
    refresh();
  }

  return { items, state, refresh, retry };
}
