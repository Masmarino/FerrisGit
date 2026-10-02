import { ChangeDetectionStrategy, Component } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TranslocoPipe } from '@jsverse/transloco'
import { injectActiveLang } from '../../i18n/active-lang'
import {
  APP_URL,
  ARTIFERRIS_URL,
  DOCS,
  GITHUB_URL,
  MILESTONES_URL,
  README_LICENSE_URL,
} from '../../shared/links'

@Component({
  selector: 'app-footer',
  imports: [RouterLink, TranslocoPipe],
  templateUrl: './footer.html',
  styleUrl: './footer.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Footer {
  protected readonly lang = injectActiveLang()

  protected readonly appUrl = APP_URL
  protected readonly docs = DOCS
  protected readonly githubUrl = GITHUB_URL
  protected readonly milestonesUrl = MILESTONES_URL
  protected readonly artiferrisUrl = ARTIFERRIS_URL
  protected readonly licenseUrl = README_LICENSE_URL
}
