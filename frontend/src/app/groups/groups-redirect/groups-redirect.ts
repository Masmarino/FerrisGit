import { Component, OnInit, inject } from '@angular/core';
import { Router } from '@angular/router';

@Component({ selector: 'fg-groups-redirect', standalone: true, template: '' })
export class GroupsRedirect implements OnInit {
  private router = inject(Router);

  ngOnInit(): void {
    this.router.navigate(['/repositories'], { queryParams: { tab: 'groups' }, replaceUrl: true });
  }
}
