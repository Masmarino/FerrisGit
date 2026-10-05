import { ActivatedRouteSnapshot } from '@angular/router';

/** One step above a page in the bar: a label, and a link when that step is a page of its own. */
export interface Crumb {
  label: string;
  link?: string[];
}

/**
 * What sits above the current page, declared on its route as `data: { trail: [...] }`: the bar shows it, and the page's
 * own title names the page. The deepest route that declares one wins; a page with none is at the top of its rail.
 */
export function pageTrail(root: ActivatedRouteSnapshot): Crumb[] {
  let trail: Crumb[] = [];
  for (let route: ActivatedRouteSnapshot | null = root; route; route = route.firstChild) {
    const declared = route.data['trail'] as Crumb[] | undefined;
    if (declared) trail = declared;
  }
  return trail;
}

/** The admin pages sit under the rail's Admin group. */
export const ADMIN_TRAIL: Crumb[] = [{ label: 'Administration' }];
