import { Lang } from '../i18n/languages'

/** Pages that show the architecture plan. Its circles link to different anchors on each. */
export type PlanPage = 'home' | 'features'

/** Where each of the seven circles leads: tiles on the home page, sections on the Product page. */
export const PLAN_FRAGMENTS: Record<PlanPage, readonly string[]> = {
  home: ['d1', 'd2', 'd3', 'd4', 'd5', 'd6', 'd7'],
  features: [
    'feature-repos',
    'feature-mrs',
    'feature-issues',
    'feature-public',
    'feature-public',
    'feature-ci',
    'feature-webhooks',
  ],
}

/** Router commands for the plan's page, in a language. */
export function planPageCommands(page: PlanPage, lang: Lang): string[] {
  return page === 'features' ? ['/', lang, 'features'] : ['/', lang]
}
