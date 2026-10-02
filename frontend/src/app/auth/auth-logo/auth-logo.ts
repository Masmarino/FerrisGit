import { Component } from '@angular/core';

/** The wordmark is black, so dark mode gets the light variant. The host is a `picture` so the panel can size the `img` directly. */
@Component({
  selector: 'picture[fgAuthLogo]',
  standalone: true,
  template: `
    <source srcset="Logo_horizontal_dark.png" media="(prefers-color-scheme: dark)" />
    <img src="Logo_horizontal.png" alt="FerrisGit" width="1762" height="592" />
  `,
})
export class AuthLogo {}
