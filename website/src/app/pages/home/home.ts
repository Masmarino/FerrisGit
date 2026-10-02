import { ChangeDetectionStrategy, Component } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TranslocoPipe } from '@jsverse/transloco'
import { Button, CopyButton } from '@masmarino/gabarit'
import { injectActiveLang } from '../../i18n/active-lang'
import { usePageMeta } from '../../seo/page-meta'
import { Capabilities } from '../../shared/capabilities/capabilities'
import { HeroScene } from '../../shared/hero-scene/hero-scene'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { Install } from '../../shared/install/install'
import { APP_URL, DOCS, GITHUB_URL, MILESTONES_URL } from '../../shared/links'
import { Parallax } from '../../shared/motion/parallax.directive'
import { Reveal } from '../../shared/motion/reveal.directive'
import { PipelineSim } from '../../shared/pipeline-sim/pipeline-sim'
import { PlanFigure } from '../../shared/plan-figure/plan-figure'
import { HERO_COMMAND } from '../../shared/snippets'

@Component({
  selector: 'app-home',
  imports: [
    RouterLink,
    TranslocoPipe,
    Button,
    CopyButton,
    Capabilities,
    HeroScene,
    InlineCodePipe,
    Install,
    Parallax,
    PipelineSim,
    PlanFigure,
    Reveal,
  ],
  templateUrl: './home.html',
  styleUrl: './home.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Home {
  protected readonly lang = injectActiveLang()

  protected readonly appUrl = APP_URL
  protected readonly githubUrl = GITHUB_URL
  protected readonly milestonesUrl = MILESTONES_URL
  protected readonly docs = DOCS
  protected readonly heroCommand = HERO_COMMAND

  // Texts are under home.facts.<id>, home.made.crates.<id>, home.security.<id> and home.roadmap.<id>.
  protected readonly facts = ['binary', 'postgres', 'crates', 'routes', 'mfa', 'engines'] as const
  protected readonly crates = ['domain', 'application', 'infrastructure', 'api', 'runner'] as const
  protected readonly security = [
    'mfa',
    'passwords',
    'secrets',
    'tokens',
    'audit',
    'cookies',
    'data',
  ] as const
  protected readonly versions = ['v02', 'v03', 'v04', 'v05', 'v06'] as const

  constructor() {
    usePageMeta('home')
  }
}
