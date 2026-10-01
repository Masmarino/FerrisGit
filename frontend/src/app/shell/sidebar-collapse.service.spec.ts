import { TestBed } from '@angular/core/testing';
import { SidebarCollapseService } from './sidebar-collapse.service';

describe('SidebarCollapseService', () => {
  beforeEach(() => {
    localStorage.clear();
    TestBed.configureTestingModule({ providers: [SidebarCollapseService] });
  });

  it('is expanded by default', () => {
    const service = TestBed.inject(SidebarCollapseService);
    expect(service.collapsed()).toBe(false);
  });

  it('persists the collapsed state to localStorage', () => {
    const service = TestBed.inject(SidebarCollapseService);
    service.set(true);

    expect(service.collapsed()).toBe(true);
    expect(localStorage.getItem('ferrisgit_sidebar_collapsed')).toBe('true');
  });

  it('restores a persisted collapsed state on injection', () => {
    localStorage.setItem('ferrisgit_sidebar_collapsed', 'true');
    const service = TestBed.inject(SidebarCollapseService);

    expect(service.collapsed()).toBe(true);
  });
});
