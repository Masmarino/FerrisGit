import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { ActivatedRoute, UrlSegment } from '@angular/router';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryPathResolver } from './repository-path-resolver';
import { RepositoriesService } from '../repositories.service';
import { withRouterAndIcons } from '../repository-story-fixtures';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';

const withPath = (resolve: RepositoriesService['resolve']) =>
  moduleMetadata({
    providers: [
      { provide: ActivatedRoute, useValue: { url: of(['repositories', 'acme', 'supprime'].map((path) => new UrlSegment(path, {}))), snapshot: { fragment: null } } },
      { provide: RepositoriesService, useValue: { resolve } },
    ],
  });

const meta: Meta<RepositoryPathResolver> = {
  title: 'Repositories/RepositoryPathResolver',
  component: RepositoryPathResolver,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<RepositoryPathResolver>;

export const NotFound: Story = {
  decorators: [withPath(() => throwError(() => new HttpErrorResponse({ status: 404, statusText: 'Not Found' })))],
};

export const Loading: Story = {
  decorators: [withPath(() => NEVER)],
};
