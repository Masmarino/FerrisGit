import { ChangeDetectionStrategy, Component } from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { usePageMeta } from '../../seo/page-meta'
import { Cta } from '../../shared/cta/cta'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { DOCS } from '../../shared/links'
import { PageHead } from '../../shared/page-head/page-head'
import { PipelineSim } from '../../shared/pipeline-sim/pipeline-sim'

import { Reveal } from '../../shared/motion/reveal.directive'

@Component({
  selector: 'app-ci',
  imports: [Reveal, TranslocoPipe, Cta, InlineCodePipe, PageHead, PipelineSim],
  templateUrl: './ci.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Ci {
  protected readonly docs = DOCS

  // Texts are under home.ci.<id>.
  protected readonly facts = ['engines', 'errors', 'vars'] as const

  constructor() {
    usePageMeta('ci')
  }
}
