import { ChangeDetectionStrategy, Component } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TranslocoPipe } from '@jsverse/transloco'
import { Button } from '@masmarino/gabarit'
import { LANGS, LANG_NAMES } from '../../i18n/languages'
import { Footer } from '../../layout/footer/footer'
import { useNotFoundMeta } from '../../seo/page-meta'

/** Prerendered as 404.html, in English: nginx serves it for any address that does not exist, in any language. */
@Component({
  selector: 'app-not-found',
  imports: [RouterLink, TranslocoPipe, Button, Footer],
  templateUrl: './not-found.html',
  styleUrl: './not-found.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class NotFound {
  protected readonly langs = LANGS
  protected readonly names = LANG_NAMES

  constructor() {
    useNotFoundMeta()
  }
}
