import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { provideRouter } from '@angular/router';
import { GroupsRedirect } from './groups-redirect';

describe('GroupsRedirect', () => {
  it('navigates to /repositories with the Groupes tab selected', () => {
    TestBed.configureTestingModule({ providers: [provideRouter([])] });
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');
    const fixture = TestBed.createComponent(GroupsRedirect);
    fixture.detectChanges();

    expect(navigateSpy).toHaveBeenCalledWith(['/repositories'], { queryParams: { tab: 'groups' }, replaceUrl: true });
  });
});
