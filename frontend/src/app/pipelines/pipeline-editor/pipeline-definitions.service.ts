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

export type PackageManager = 'npm' | 'pnpm' | 'yarnClassic' | 'yarn' | 'bun';
export type NodeFramework = 'angular' | 'react' | 'vue' | 'svelte' | 'next';
export type TestRunner = 'vitest' | 'jest' | 'karma' | 'playwright';
export type PythonTool = 'pip' | 'poetry' | 'uv';

interface ProjectBase {
  /** The project's folder, `''` for the root. */
  dir: string;
  /** The files it was recognised by. */
  evidence: string[];
}

export type DetectedProject =
  | (ProjectBase & { kind: 'rust'; workspace: boolean; toolchain: string | null; sqlxOffline: boolean; sqlxPostgres: boolean })
  | (ProjectBase & { kind: 'node'; packageManager: PackageManager; nodeVersion: string | null; scripts: Record<string, string>; framework: NodeFramework | null; testRunner: TestRunner | null })
  | (ProjectBase & { kind: 'go'; goVersion: string | null })
  | (ProjectBase & { kind: 'python'; tool: PythonTool; pythonVersion: string | null; pytest: boolean; ruff: boolean });

/**
 * What the default branch is made of, as the server read it: the editor's predictions start from here. These types
 * mirror repository_profile.rs; repository-profile.contract.spec.ts checks them against a sample the server's own tests
 * keep up to date.
 */
export interface RepositoryProfile {
  projects: DetectedProject[];
  /** Folders with a Dockerfile, `''` for the root. */
  dockerfiles: string[];
  /** Folders with a Helm chart. */
  helmCharts: string[];
}

/** The pipeline builder's calls: both are done by the server's own parser, so the builder cannot disagree with it. */
@Injectable({ providedIn: 'root' })
export class PipelineDefinitionsService {
  private http = inject(HttpClient);

  repositoryFile(repositoryId: string): Observable<RepositoryPipelineFile> {
    return this.http.get<RepositoryPipelineFile>(`/api/repositories/${repositoryId}/pipeline-definition`);
  }

  /** What the repository is made of, to propose a pipeline that fits it. */
  repositoryProfile(repositoryId: string): Observable<RepositoryProfile> {
    return this.http.get<RepositoryProfile>(`/api/repositories/${repositoryId}/pipeline-definition/profile`);
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
