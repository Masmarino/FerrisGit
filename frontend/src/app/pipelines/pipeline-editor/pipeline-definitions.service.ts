import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';

export interface JobDto {
  stage: string;
  image: string;
  script: string[];
  variables: Record<string, string>;
  needs: string[];
  tags: string[];
  cache: string[];
}

/** A pipeline as the server's parser reads it: stages in order, jobs by name. */
export interface DefinitionDto {
  stages: string[];
  jobs: Record<string, JobDto>;
}

/** What the server found wrong, with a stable `code` and the names it is about. `message` is the parser's own English. */
export interface PipelineProblem {
  code: string;
  message: string;
  job?: string;
  stage?: string;
  dependency?: string;
  key?: string;
  jobs?: string[];
}

/** Something the server accepts but that is probably a mistake. */
export interface PipelineWarning {
  code: string;
  job?: string;
  stage?: string;
}

export interface ParsedPipeline {
  /** `null` when the YAML itself is malformed. A file with wrong stages or dependencies still has one. */
  definition: DefinitionDto | null;
  problems: PipelineProblem[];
  warnings: PipelineWarning[];
  /** Fields of the file that a rewrite would drop, as dotted paths. */
  ignoredFields: string[];
  hasComments: boolean;
}

export interface RenderedPipeline {
  yaml: string;
  problems: PipelineProblem[];
  warnings: PipelineWarning[];
}

/** The repository's pipeline file as its default branch has it. A repository with no commit has neither branch nor file. */
export interface RepositoryPipelineFile {
  /** Where the repository keeps it: a repository setting, `.ferrisgit-ci.yml` unless changed. */
  path: string;
  branch: string | null;
  /** The branch's tip the file was read at: saving checks the file did not change since. */
  baseSha: string | null;
  yaml: string | null;
}

export interface PipelineProposalRequest {
  yaml: string;
  baseSha: string;
  title: string;
  description: string;
}

/** The new branch and the merge request that holds the change. */
export interface PipelineProposal {
  branch: string;
  commitSha: string;
  mergeRequestId: string;
}

/** The pipeline builder's calls: both are done by the server's own parser, so the builder cannot disagree with it. */
@Injectable({ providedIn: 'root' })
export class PipelineDefinitionsService {
  private http = inject(HttpClient);

  repositoryFile(repositoryId: string): Observable<RepositoryPipelineFile> {
    return this.http.get<RepositoryPipelineFile>(`/api/repositories/${repositoryId}/pipeline-definition`);
  }

  /** Saves on a new branch and opens a merge request: the default branch is never written to. */
  propose(repositoryId: string, request: PipelineProposalRequest): Observable<PipelineProposal> {
    return this.http.post<PipelineProposal>(`/api/repositories/${repositoryId}/pipeline-definition/proposal`, request);
  }

  parse(yaml: string): Observable<ParsedPipeline> {
    return this.http.post<ParsedPipeline>('/api/pipeline-definitions/parse', { yaml });
  }

  render(definition: DefinitionDto): Observable<RenderedPipeline> {
    return this.http.post<RenderedPipeline>('/api/pipeline-definitions/render', { definition });
  }
}
