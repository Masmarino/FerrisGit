import { inject, signal } from '@angular/core';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Observable } from 'rxjs';
import { t } from '../shared/i18n/translator';

/** If a refresh fails after a first load, the old list stays and a toast reports it. Needs an injection context. */
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
        toast.show(t('repositories.loadSettingsFailed'), 'error');
      },
    });
  }

  function retry(): void {
    state.set('loading');
    refresh();
  }

  return { items, state, refresh, retry };
}
