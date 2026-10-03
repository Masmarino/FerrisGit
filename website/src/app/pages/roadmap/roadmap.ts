import { ChangeDetectionStrategy, Component } from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { Button } from '@masmarino/gabarit'
import { usePageMeta } from '../../seo/page-meta'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { ARTIFERRIS_URL, MILESTONES_URL, README_ROADMAP_URL } from '../../shared/links'
import { PageHead } from '../../shared/page-head/page-head'

interface RoadmapGroup {
  id: string
  /** The version, shown as a mono mark in front of the title. Groups without one are not scheduled. */
  tag?: string
  items: number
  hasIntro: boolean
  /** "Planned" for a scheduled version or list, "Under consideration" for ideas with no version. */
  status: 'planned' | 'considering'
}

// Mirrors the Roadmap section of the README; the texts are under roadmap.groups.<id>.
const GROUPS: RoadmapGroup[] = [
  { id: 'v02', tag: '0.2', items: 9, hasIntro: true, status: 'planned' },
  { id: 'v03', tag: '0.3', items: 5, hasIntro: true, status: 'planned' },
  { id: 'v04', tag: '0.4', items: 4, hasIntro: false, status: 'planned' },
  { id: 'v05', tag: '0.5', items: 4, hasIntro: true, status: 'planned' },
  { id: 'v06', tag: '0.6', items: 6, hasIntro: false, status: 'planned' },
  { id: 'platform', items: 8, hasIntro: false, status: 'planned' },
  { id: 'consideration', items: 3, hasIntro: true, status: 'considering' },
]

import { Reveal } from '../../shared/motion/reveal.directive'

@Component({
  selector: 'app-roadmap',
  imports: [Reveal, TranslocoPipe, Button, InlineCodePipe, PageHead],
  templateUrl: './roadmap.html',
  styleUrl: './roadmap.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Roadmap {
  protected readonly groups = GROUPS.map((group) => ({
    ...group,
    numbers: Array.from({ length: group.items }, (_, index) => index + 1),
  }))
  protected readonly milestonesUrl = MILESTONES_URL
  protected readonly readmeUrl = README_ROADMAP_URL
  protected readonly artiferrisUrl = ARTIFERRIS_URL

  constructor() {
    usePageMeta('roadmap')
  }
}
