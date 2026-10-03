import { ChangeDetectionStrategy, Component } from '@angular/core'
import { TranslocoPipe } from '@jsverse/transloco'
import { usePageMeta } from '../../seo/page-meta'
import { Cta } from '../../shared/cta/cta'
import { InlineCodePipe } from '../../shared/inline-code.pipe'
import { DOCS } from '../../shared/links'
import { PageHead } from '../../shared/page-head/page-head'
import { SECURITY_HEADERS } from '../../shared/snippets'
import { Terminal } from '../../shared/terminal/terminal'

import { Reveal } from '../../shared/motion/reveal.directive'

@Component({
  selector: 'app-security',
  imports: [Reveal, TranslocoPipe, Cta, InlineCodePipe, PageHead, Terminal],
  templateUrl: './security.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Security {
  protected readonly docs = DOCS
  protected readonly headers = SECURITY_HEADERS

  // Texts are under home.security.<id>.
  protected readonly facts = [
    'mfa',
    'passwords',
    'secrets',
    'tokens',
    'audit',
    'cookies',
    'data',
  ] as const

  constructor() {
    usePageMeta('security')
  }
}
