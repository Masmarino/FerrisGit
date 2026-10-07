import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { PipelineDefinitionsService } from './pipeline-definitions.service';

describe('PipelineDefinitionsService', () => {
  beforeEach(() => TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] }));

  it('sends the YAML to be read by the server', () => {
    const http = TestBed.inject(HttpTestingController);
    let result: unknown;

    TestBed.inject(PipelineDefinitionsService).parse('stages: [a]').subscribe((r) => (result = r));
    const request = http.expectOne('/api/pipeline-definitions/parse');
    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({ yaml: 'stages: [a]' });
    request.flush({ definition: null, problems: [], warnings: [], ignoredFields: [], hasComments: false });

    expect(result).toEqual({ definition: null, problems: [], warnings: [], ignoredFields: [], hasComments: false });
  });

  it('sends the definition to be written as YAML', () => {
    const http = TestBed.inject(HttpTestingController);
    const definition = { stages: ['build'], jobs: {} };
    let yaml = '';

    TestBed.inject(PipelineDefinitionsService)
      .render(definition)
      .subscribe((r) => (yaml = r.yaml));
    const request = http.expectOne('/api/pipeline-definitions/render');
    expect(request.request.body).toEqual({ definition });
    request.flush({ yaml: 'stages:\n- build\njobs: {}\n', problems: [], warnings: [] });

    expect(yaml).toBe('stages:\n- build\njobs: {}\n');
  });
});
