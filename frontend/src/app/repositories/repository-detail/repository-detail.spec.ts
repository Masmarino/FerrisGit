import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { RepositoryDetail } from './repository-detail';

describe('RepositoryDetail', () => {
  it('renders RepositoryTreeView at ref HEAD and the repository root', () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])] });
    const fixture = TestBed.createComponent(RepositoryDetail);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    fixture.detectChanges();

    const treeView = fixture.nativeElement.querySelector('fg-repository-tree-view');
    expect(treeView).toBeTruthy();

    TestBed.inject(HttpTestingController).match(() => true).forEach((req) => req.flush([]));
  });
});
