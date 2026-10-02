// Spec-only: the setup every repository settings section shares.
import { Provider, Type } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { GbtToastService } from '@masmarino/gabarit';

/** Renders a settings section for `repo-1` with the given service stubs and a toast stub. */
export function createSettingsSection<T>(component: Type<T>, providers: Provider[]) {
  const toastStub = { show: vi.fn() };
  TestBed.configureTestingModule({ providers: [...providers, { provide: GbtToastService, useValue: toastStub }] });
  const fixture = TestBed.createComponent(component);
  fixture.componentRef.setInput('repositoryId', 'repo-1');
  return { fixture, component: fixture.componentInstance, toastStub };
}
