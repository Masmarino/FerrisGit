import { Component } from '@angular/core';

/** The wordmark is black, so the dark theme uses the light variant. The host is a `picture` (no wrapper element) so the panel sizes the `img` directly. */
@Component({
  selector: 'picture[fgAuthLogo]',
  standalone: true,
  template: `
    <source srcset="Logo_horizontal_dark.png" media="(prefers-color-scheme: dark)" />
    <img src="Logo_horizontal.png" alt="FerrisGit" width="1762" height="592" />
  `,
})
export class AuthLogo {}
