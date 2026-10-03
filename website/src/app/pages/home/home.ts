import { ChangeDetectionStrategy, Component } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TranslocoPipe } from '@jsverse/transloco'
import { Button, CopyButton } from '@masmarino/gabarit'
import { injectActiveLang } from '../../i18n/active-lang'
import { usePageMeta } from '../../seo/page-meta'
import { Capabilities } from '../../shared/capabilities/capabilities'
import { Cta } from '../../shared/cta/cta'
import { GitField } from '../../shared/git-field/git-field'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { APP_URL, DOCS, GITHUB_URL, MILESTONES_URL } from '../../shared/links'
import { PlanFigure } from '../../shared/plan-figure/plan-figure'
import { HERO_COMMAND, PROOF_TERMINAL } from '../../shared/snippets'

import { Reveal } from '../../shared/motion/reveal.directive'
import { Terminal } from '../../shared/terminal/terminal'

@Component({
  selector: 'app-home',
  imports: [
    GitField,
    Reveal,
    RouterLink,
    TranslocoPipe,
    Button,
    CopyButton,
    Capabilities,
    InlineCodePipe,
    Cta,
    PlanFigure,
    Terminal,
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
  protected readonly proofTerminal = PROOF_TERMINAL

  // Texts are under home.why.<id>, home.limits.<id>, home.made.crates.<id> and home.roadmap.<id>.
  protected readonly why = ['small', 'mfa', 'admin', 'ci'] as const
  protected readonly limits = ['ssh', 'branches', 'language', 'analysis'] as const
  protected readonly crates = ['domain', 'application', 'infrastructure', 'api', 'runner'] as const
  protected readonly versions = ['v02', 'v03', 'v04', 'v05', 'v06'] as const

  constructor() {
    usePageMeta('home')
  }
}
