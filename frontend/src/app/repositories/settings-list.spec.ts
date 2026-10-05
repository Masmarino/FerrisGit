import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Observable, of, throwError } from 'rxjs';
import { createSettingsList } from './settings-list';

describe('createSettingsList', () => {
  function setup() {
    let response: Observable<string[]> = of(['a']);
    const toast = { show: vi.fn() };
    TestBed.configureTestingModule({ providers: [{ provide: GbtToastService, useValue: toast }] });

    @Component({ selector: 'fg-settings-list-host', template: '' })
    class Host {
      list = createSettingsList(() => response);
    }
    const { list } = TestBed.createComponent(Host).componentInstance;
    return { list, toast, respond: (next: Observable<string[]>) => (response = next) };
  }

  it('starts loading, then holds the first list', () => {
    const { list } = setup();
    expect(list.state()).toBe('loading');

    list.refresh();

    expect(list.state()).toBe('loaded');
    expect(list.items()).toEqual(['a']);
  });

  it('fails when the first load fails, and goes back to loading on retry', () => {
    const { list, toast, respond } = setup();
    respond(throwError(() => new Error('500')));

    list.refresh();

    expect(list.state()).toBe('failed');
    expect(toast.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');

    respond(of(['b']));
    list.retry();

    expect(list.state()).toBe('loaded');
    expect(list.items()).toEqual(['b']);
  });

  it('keeps the previous list on screen when a later refresh fails', () => {
    const { list, toast, respond } = setup();
    list.refresh();
    respond(throwError(() => new Error('500')));

    list.refresh();

    expect(list.state()).toBe('loaded');
    expect(list.items()).toEqual(['a']);
    expect(toast.show).toHaveBeenCalledTimes(1);
  });
});
