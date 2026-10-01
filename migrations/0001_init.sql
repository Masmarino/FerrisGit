-- FerrisGit database schema, squashed from the ten incremental migrations that preceded the first production reset.
-- Each section below is one of them, in order (0001 to 0010). The data-only statements they carried (the
-- merge-request event backfill and the session-revoking token_epoch bump) are gone: they only mattered for rows
-- that existed before those migrations ran, and this one always starts on an empty database.

-- ============================================================================
-- 0001_init
-- ============================================================================

-- migrations/0001_init.sql
--
-- Squashed schema: this single migration replaces what used to be 32 separate
-- incremental migrations (0001_init.sql through 0032_user_token_epoch.sql).
-- Generated via `pg_dump --schema-only` against a database with all 32 of the
-- original migrations applied, so it produces the exact same final schema —
-- just without the intermediate history (tables created then altered, an
-- index created then dropped a migration later, etc).
--
-- Existing databases that already have the old 32-migration history applied
-- must NOT run this file — their `_sqlx_migrations` table already reflects
-- that schema. This squash is meant for fresh databases going forward.

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: api_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.api_tokens (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    token_hash text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone
);


--
-- Name: domain_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.domain_events (
    id bigint NOT NULL,
    aggregate_type text NOT NULL,
    aggregate_id text NOT NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL,
    version bigint DEFAULT 1 NOT NULL,
    actor_id uuid,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: domain_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.domain_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: domain_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.domain_events_id_seq OWNED BY public.domain_events.id;


--
-- Name: group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_members (
    group_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT group_members_role_check CHECK ((role = ANY (ARRAY['reader'::text, 'contributor'::text, 'maintainer'::text])))
);


--
-- Name: groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.groups (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    parent_group_id uuid,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT groups_name_check CHECK ((name ~ '^[a-zA-Z0-9_-]+$'::text))
);


--
-- Name: issue_comments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.issue_comments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    issue_id uuid NOT NULL,
    author_id uuid NOT NULL,
    body text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: issue_labels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.issue_labels (
    issue_id uuid NOT NULL,
    label_id uuid NOT NULL
);


--
-- Name: issues; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.issues (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    number integer NOT NULL,
    author_id uuid NOT NULL,
    assignee_id uuid,
    title text NOT NULL,
    description text NOT NULL,
    status text NOT NULL,
    kind text NOT NULL,
    parent_issue_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    closed_at timestamp with time zone,
    search_vector tsvector GENERATED ALWAYS AS ((setweight(to_tsvector('simple'::regconfig, title), 'A'::"char") || setweight(to_tsvector('simple'::regconfig, description), 'B'::"char"))) STORED,
    milestone_id uuid
);


--
-- Name: jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    pipeline_id uuid NOT NULL,
    stage text NOT NULL,
    name text NOT NULL,
    image text NOT NULL,
    script jsonb NOT NULL,
    variables jsonb DEFAULT '{}'::jsonb NOT NULL,
    needs text[] DEFAULT '{}'::text[] NOT NULL,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    runner_id uuid,
    logs text DEFAULT ''::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    cache text[] DEFAULT '{}'::text[] NOT NULL,
    CONSTRAINT jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'success'::text, 'failed'::text, 'canceled'::text])))
);


--
-- Name: labels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.labels (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    color text NOT NULL,
    repository_id uuid,
    group_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT labels_scope_xor CHECK ((num_nonnulls(repository_id, group_id) = 1))
);


--
-- Name: merge_request_comments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.merge_request_comments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    merge_request_id uuid NOT NULL,
    author_id uuid NOT NULL,
    body text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    reply_to_id uuid,
    file_path text,
    line_number integer,
    side text,
    anchor_content text,
    resolved boolean DEFAULT false NOT NULL,
    end_line integer,
    suggested_content text,
    applied_at timestamp with time zone,
    applied_commit_sha text,
    CONSTRAINT merge_request_comments_anchor_all_or_nothing CHECK ((((file_path IS NULL) AND (line_number IS NULL) AND (side IS NULL) AND (anchor_content IS NULL)) OR ((file_path IS NOT NULL) AND (line_number IS NOT NULL) AND (side IS NOT NULL) AND (anchor_content IS NOT NULL)))),
    CONSTRAINT merge_request_comments_applied_requires_suggestion CHECK ((((applied_at IS NULL) AND (applied_commit_sha IS NULL)) OR (suggested_content IS NOT NULL))),
    CONSTRAINT merge_request_comments_end_line_after_start CHECK (((end_line IS NULL) OR (end_line >= line_number))),
    CONSTRAINT merge_request_comments_range_requires_anchor CHECK ((((end_line IS NULL) AND (suggested_content IS NULL)) OR (line_number IS NOT NULL))),
    CONSTRAINT merge_request_comments_side_check CHECK ((side = ANY (ARRAY['old'::text, 'new'::text])))
);


--
-- Name: merge_request_labels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.merge_request_labels (
    merge_request_id uuid NOT NULL,
    label_id uuid NOT NULL
);


--
-- Name: merge_request_reviews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.merge_request_reviews (
    merge_request_id uuid NOT NULL,
    user_id uuid NOT NULL,
    decision text NOT NULL,
    source_sha text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT merge_request_reviews_decision_check CHECK ((decision = ANY (ARRAY['approved'::text, 'changes_requested'::text])))
);


--
-- Name: merge_requests; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.merge_requests (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    author_id uuid NOT NULL,
    source_branch text NOT NULL,
    target_branch text NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    merge_commit_sha text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    closed_at timestamp with time zone,
    search_vector tsvector GENERATED ALWAYS AS ((setweight(to_tsvector('simple'::regconfig, title), 'A'::"char") || setweight(to_tsvector('simple'::regconfig, description), 'B'::"char"))) STORED,
    milestone_id uuid,
    CONSTRAINT merge_requests_status_check CHECK ((status = ANY (ARRAY['open'::text, 'merged'::text, 'closed'::text])))
);


--
-- Name: milestones; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.milestones (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    due_date timestamp with time zone,
    state text DEFAULT 'open'::text NOT NULL,
    repository_id uuid,
    group_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT milestones_scope_xor CHECK ((num_nonnulls(repository_id, group_id) = 1))
);


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    recipient_id uuid NOT NULL,
    kind text NOT NULL,
    repository_owner text NOT NULL,
    repository_name text NOT NULL,
    actor_username text,
    merge_request_id uuid,
    merge_request_title text,
    pipeline_id uuid,
    commit_sha text,
    role text,
    read_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    issue_id uuid,
    issue_number integer,
    issue_title text
);


--
-- Name: pipelines; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pipelines (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    commit_sha text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    execution_engine text DEFAULT 'docker-runners'::text NOT NULL,
    triggered_by uuid NOT NULL,
    CONSTRAINT pipelines_execution_engine_check CHECK ((execution_engine = ANY (ARRAY['docker-runners'::text, 'kubernetes'::text]))),
    CONSTRAINT pipelines_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'success'::text, 'failed'::text, 'canceled'::text])))
);


--
-- Name: release_assets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.release_assets (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    release_id uuid NOT NULL,
    filename text NOT NULL,
    content_type text NOT NULL,
    size_bytes bigint NOT NULL,
    disk_path text NOT NULL,
    uploaded_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: releases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.releases (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    tag_name text NOT NULL,
    title text NOT NULL,
    notes text DEFAULT ''::text NOT NULL,
    draft boolean DEFAULT false NOT NULL,
    prerelease boolean DEFAULT false NOT NULL,
    author_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    published_at timestamp with time zone
);


--
-- Name: repositories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repositories (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    owner_id uuid NOT NULL,
    name text NOT NULL,
    disk_path text NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    next_issue_number integer DEFAULT 1 NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    group_id uuid,
    search_vector tsvector GENERATED ALWAYS AS ((setweight(to_tsvector('simple'::regconfig, name), 'A'::"char") || setweight(to_tsvector('simple'::regconfig, description), 'B'::"char"))) STORED,
    CONSTRAINT repositories_visibility_check CHECK ((visibility = ANY (ARRAY['private'::text, 'public'::text])))
);


--
-- Name: repository_ci_variables; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_ci_variables (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    key text NOT NULL,
    encrypted_value bytea NOT NULL,
    masked boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: repository_collaborators; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_collaborators (
    repository_id uuid NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    role text DEFAULT 'contributor'::text NOT NULL,
    CONSTRAINT repository_collaborators_role_check CHECK ((role = ANY (ARRAY['reader'::text, 'contributor'::text, 'maintainer'::text])))
);


--
-- Name: repository_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_settings (
    repository_id uuid NOT NULL,
    pipeline_file_path text DEFAULT '.ferrisgit-ci.yml'::text NOT NULL,
    ci_enabled boolean DEFAULT true NOT NULL,
    required_approvals integer DEFAULT 0 NOT NULL
);


--
-- Name: repository_stars; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.repository_stars (
    repository_id uuid NOT NULL,
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: runners; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.runners (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    token_hash text NOT NULL,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    last_heartbeat_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: system_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.system_settings (
    id boolean DEFAULT true NOT NULL,
    execution_engine text DEFAULT 'docker-runners'::text NOT NULL,
    k8s_namespace text,
    k8s_cache_storage_class text,
    runner_registration_token text,
    log_retention_days integer,
    max_concurrent_jobs integer,
    jwt_ttl_hours integer DEFAULT 12 NOT NULL,
    max_push_size_mb integer DEFAULT 500 NOT NULL,
    CONSTRAINT jwt_ttl_hours_range CHECK (((jwt_ttl_hours >= 1) AND (jwt_ttl_hours <= 720))),
    CONSTRAINT system_settings_execution_engine_check CHECK ((execution_engine = ANY (ARRAY['docker-runners'::text, 'kubernetes'::text]))),
    CONSTRAINT system_settings_id_check CHECK ((id = true))
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    username text NOT NULL,
    email text NOT NULL,
    password_hash text NOT NULL,
    is_admin boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    search_vector tsvector GENERATED ALWAYS AS (setweight(to_tsvector('simple'::regconfig, username), 'A'::"char")) STORED,
    token_epoch integer DEFAULT 0 NOT NULL
);


--
-- Name: webhook_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhook_deliveries (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    webhook_id uuid NOT NULL,
    event_kind text NOT NULL,
    http_status integer,
    success boolean NOT NULL,
    error_message text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: webhooks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webhooks (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    url text NOT NULL,
    secret_ciphertext bytea NOT NULL,
    events text[] NOT NULL,
    active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: wikis; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.wikis (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    repository_id uuid NOT NULL,
    disk_path text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: domain_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.domain_events ALTER COLUMN id SET DEFAULT nextval('public.domain_events_id_seq'::regclass);


--
-- Name: api_tokens api_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.api_tokens
    ADD CONSTRAINT api_tokens_pkey PRIMARY KEY (id);


--
-- Name: api_tokens api_tokens_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.api_tokens
    ADD CONSTRAINT api_tokens_token_hash_key UNIQUE (token_hash);


--
-- Name: domain_events domain_events_aggregate_type_aggregate_id_version_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.domain_events
    ADD CONSTRAINT domain_events_aggregate_type_aggregate_id_version_key UNIQUE (aggregate_type, aggregate_id, version);


--
-- Name: domain_events domain_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.domain_events
    ADD CONSTRAINT domain_events_pkey PRIMARY KEY (id);


--
-- Name: group_members group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (group_id, user_id);


--
-- Name: groups groups_parent_group_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_parent_group_id_name_key UNIQUE (parent_group_id, name);


--
-- Name: groups groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (id);


--
-- Name: issue_comments issue_comments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_pkey PRIMARY KEY (id);


--
-- Name: issue_labels issue_labels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_labels
    ADD CONSTRAINT issue_labels_pkey PRIMARY KEY (issue_id, label_id);


--
-- Name: issues issues_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_pkey PRIMARY KEY (id);


--
-- Name: issues issues_repository_id_number_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_repository_id_number_key UNIQUE (repository_id, number);


--
-- Name: jobs jobs_pipeline_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_pipeline_id_name_key UNIQUE (pipeline_id, name);


--
-- Name: jobs jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_pkey PRIMARY KEY (id);


--
-- Name: labels labels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels
    ADD CONSTRAINT labels_pkey PRIMARY KEY (id);


--
-- Name: merge_request_comments merge_request_comments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_comments
    ADD CONSTRAINT merge_request_comments_pkey PRIMARY KEY (id);


--
-- Name: merge_request_labels merge_request_labels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_labels
    ADD CONSTRAINT merge_request_labels_pkey PRIMARY KEY (merge_request_id, label_id);


--
-- Name: merge_request_reviews merge_request_reviews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_reviews
    ADD CONSTRAINT merge_request_reviews_pkey PRIMARY KEY (merge_request_id, user_id);


--
-- Name: merge_requests merge_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_requests
    ADD CONSTRAINT merge_requests_pkey PRIMARY KEY (id);


--
-- Name: milestones milestones_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.milestones
    ADD CONSTRAINT milestones_pkey PRIMARY KEY (id);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: pipelines pipelines_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipelines
    ADD CONSTRAINT pipelines_pkey PRIMARY KEY (id);


--
-- Name: release_assets release_assets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.release_assets
    ADD CONSTRAINT release_assets_pkey PRIMARY KEY (id);


--
-- Name: releases releases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.releases
    ADD CONSTRAINT releases_pkey PRIMARY KEY (id);


--
-- Name: releases releases_repository_id_tag_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.releases
    ADD CONSTRAINT releases_repository_id_tag_name_key UNIQUE (repository_id, tag_name);


--
-- Name: repositories repositories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repositories
    ADD CONSTRAINT repositories_pkey PRIMARY KEY (id);


--
-- Name: repository_ci_variables repository_ci_variables_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_ci_variables
    ADD CONSTRAINT repository_ci_variables_pkey PRIMARY KEY (id);


--
-- Name: repository_ci_variables repository_ci_variables_repository_id_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_ci_variables
    ADD CONSTRAINT repository_ci_variables_repository_id_key_key UNIQUE (repository_id, key);


--
-- Name: repository_collaborators repository_collaborators_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_collaborators
    ADD CONSTRAINT repository_collaborators_pkey PRIMARY KEY (repository_id, user_id);


--
-- Name: repository_settings repository_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_settings
    ADD CONSTRAINT repository_settings_pkey PRIMARY KEY (repository_id);


--
-- Name: repository_stars repository_stars_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_stars
    ADD CONSTRAINT repository_stars_pkey PRIMARY KEY (repository_id, user_id);


--
-- Name: runners runners_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runners
    ADD CONSTRAINT runners_pkey PRIMARY KEY (id);


--
-- Name: runners runners_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.runners
    ADD CONSTRAINT runners_token_hash_key UNIQUE (token_hash);


--
-- Name: system_settings system_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.system_settings
    ADD CONSTRAINT system_settings_pkey PRIMARY KEY (id);


--
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- Name: webhook_deliveries webhook_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_pkey PRIMARY KEY (id);


--
-- Name: webhooks webhooks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_pkey PRIMARY KEY (id);


--
-- Name: wikis wikis_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wikis
    ADD CONSTRAINT wikis_pkey PRIMARY KEY (id);


--
-- Name: wikis wikis_repository_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wikis
    ADD CONSTRAINT wikis_repository_id_key UNIQUE (repository_id);


--
-- Name: groups_root_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX groups_root_name_unique ON public.groups USING btree (name) WHERE (parent_group_id IS NULL);


--
-- Name: issue_comments_issue_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX issue_comments_issue_idx ON public.issue_comments USING btree (issue_id);


--
-- Name: issues_milestone_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX issues_milestone_idx ON public.issues USING btree (milestone_id);


--
-- Name: issues_repository_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX issues_repository_idx ON public.issues USING btree (repository_id);


--
-- Name: issues_search_vector_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX issues_search_vector_idx ON public.issues USING gin (search_vector);


--
-- Name: labels_group_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX labels_group_idx ON public.labels USING btree (group_id);


--
-- Name: labels_repository_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX labels_repository_idx ON public.labels USING btree (repository_id);


--
-- Name: merge_requests_milestone_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX merge_requests_milestone_idx ON public.merge_requests USING btree (milestone_id);


--
-- Name: merge_requests_search_vector_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX merge_requests_search_vector_idx ON public.merge_requests USING gin (search_vector);


--
-- Name: milestones_group_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX milestones_group_idx ON public.milestones USING btree (group_id);


--
-- Name: milestones_repository_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX milestones_repository_idx ON public.milestones USING btree (repository_id);


--
-- Name: notifications_recipient_created_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX notifications_recipient_created_idx ON public.notifications USING btree (recipient_id, created_at DESC);


--
-- Name: notifications_recipient_unread_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX notifications_recipient_unread_idx ON public.notifications USING btree (recipient_id, read_at);


--
-- Name: release_assets_release_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX release_assets_release_id_idx ON public.release_assets USING btree (release_id);


--
-- Name: repositories_group_id_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX repositories_group_id_name_unique ON public.repositories USING btree (group_id, name) WHERE (group_id IS NOT NULL);


--
-- Name: repositories_owner_id_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX repositories_owner_id_name_unique ON public.repositories USING btree (owner_id, name) WHERE (group_id IS NULL);


--
-- Name: repositories_public_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX repositories_public_idx ON public.repositories USING btree (id) WHERE (visibility = 'public'::text);


--
-- Name: repositories_search_vector_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX repositories_search_vector_idx ON public.repositories USING gin (search_vector);


--
-- Name: users_search_vector_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX users_search_vector_idx ON public.users USING gin (search_vector);


--
-- Name: webhook_deliveries_webhook_id_created_at_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX webhook_deliveries_webhook_id_created_at_idx ON public.webhook_deliveries USING btree (webhook_id, created_at DESC);


--
-- Name: webhooks_repository_id_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX webhooks_repository_id_idx ON public.webhooks USING btree (repository_id);


--
-- Name: api_tokens api_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.api_tokens
    ADD CONSTRAINT api_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.groups(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: groups groups_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id);


--
-- Name: groups groups_parent_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_parent_group_id_fkey FOREIGN KEY (parent_group_id) REFERENCES public.groups(id) ON DELETE CASCADE;


--
-- Name: issue_comments issue_comments_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: issue_comments issue_comments_issue_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_comments
    ADD CONSTRAINT issue_comments_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;


--
-- Name: issue_labels issue_labels_issue_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_labels
    ADD CONSTRAINT issue_labels_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE CASCADE;


--
-- Name: issue_labels issue_labels_label_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issue_labels
    ADD CONSTRAINT issue_labels_label_id_fkey FOREIGN KEY (label_id) REFERENCES public.labels(id) ON DELETE CASCADE;


--
-- Name: issues issues_assignee_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_assignee_id_fkey FOREIGN KEY (assignee_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: issues issues_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: issues issues_milestone_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_milestone_id_fkey FOREIGN KEY (milestone_id) REFERENCES public.milestones(id) ON DELETE SET NULL;


--
-- Name: issues issues_parent_issue_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_parent_issue_id_fkey FOREIGN KEY (parent_issue_id) REFERENCES public.issues(id) ON DELETE SET NULL;


--
-- Name: issues issues_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.issues
    ADD CONSTRAINT issues_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: jobs jobs_pipeline_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_pipeline_id_fkey FOREIGN KEY (pipeline_id) REFERENCES public.pipelines(id) ON DELETE CASCADE;


--
-- Name: jobs jobs_runner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_runner_id_fkey FOREIGN KEY (runner_id) REFERENCES public.runners(id) ON DELETE SET NULL;


--
-- Name: labels labels_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels
    ADD CONSTRAINT labels_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.groups(id) ON DELETE CASCADE;


--
-- Name: labels labels_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.labels
    ADD CONSTRAINT labels_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: merge_request_comments merge_request_comments_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_comments
    ADD CONSTRAINT merge_request_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: merge_request_comments merge_request_comments_merge_request_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_comments
    ADD CONSTRAINT merge_request_comments_merge_request_id_fkey FOREIGN KEY (merge_request_id) REFERENCES public.merge_requests(id) ON DELETE CASCADE;


--
-- Name: merge_request_comments merge_request_comments_reply_to_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_comments
    ADD CONSTRAINT merge_request_comments_reply_to_id_fkey FOREIGN KEY (reply_to_id) REFERENCES public.merge_request_comments(id) ON DELETE SET NULL;


--
-- Name: merge_request_labels merge_request_labels_label_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_labels
    ADD CONSTRAINT merge_request_labels_label_id_fkey FOREIGN KEY (label_id) REFERENCES public.labels(id) ON DELETE CASCADE;


--
-- Name: merge_request_labels merge_request_labels_merge_request_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_labels
    ADD CONSTRAINT merge_request_labels_merge_request_id_fkey FOREIGN KEY (merge_request_id) REFERENCES public.merge_requests(id) ON DELETE CASCADE;


--
-- Name: merge_request_reviews merge_request_reviews_merge_request_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_reviews
    ADD CONSTRAINT merge_request_reviews_merge_request_id_fkey FOREIGN KEY (merge_request_id) REFERENCES public.merge_requests(id) ON DELETE CASCADE;


--
-- Name: merge_request_reviews merge_request_reviews_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_request_reviews
    ADD CONSTRAINT merge_request_reviews_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: merge_requests merge_requests_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_requests
    ADD CONSTRAINT merge_requests_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: merge_requests merge_requests_milestone_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_requests
    ADD CONSTRAINT merge_requests_milestone_id_fkey FOREIGN KEY (milestone_id) REFERENCES public.milestones(id) ON DELETE SET NULL;


--
-- Name: merge_requests merge_requests_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.merge_requests
    ADD CONSTRAINT merge_requests_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: milestones milestones_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.milestones
    ADD CONSTRAINT milestones_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.groups(id) ON DELETE CASCADE;


--
-- Name: milestones milestones_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.milestones
    ADD CONSTRAINT milestones_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: notifications notifications_issue_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_issue_id_fkey FOREIGN KEY (issue_id) REFERENCES public.issues(id) ON DELETE SET NULL;


--
-- Name: notifications notifications_recipient_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_recipient_id_fkey FOREIGN KEY (recipient_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: pipelines pipelines_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipelines
    ADD CONSTRAINT pipelines_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: pipelines pipelines_triggered_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pipelines
    ADD CONSTRAINT pipelines_triggered_by_fkey FOREIGN KEY (triggered_by) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: release_assets release_assets_release_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.release_assets
    ADD CONSTRAINT release_assets_release_id_fkey FOREIGN KEY (release_id) REFERENCES public.releases(id) ON DELETE CASCADE;


--
-- Name: release_assets release_assets_uploaded_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.release_assets
    ADD CONSTRAINT release_assets_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES public.users(id);


--
-- Name: releases releases_author_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.releases
    ADD CONSTRAINT releases_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id);


--
-- Name: releases releases_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.releases
    ADD CONSTRAINT releases_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: repositories repositories_group_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repositories
    ADD CONSTRAINT repositories_group_id_fkey FOREIGN KEY (group_id) REFERENCES public.groups(id) ON DELETE CASCADE;


--
-- Name: repositories repositories_owner_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repositories
    ADD CONSTRAINT repositories_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: repository_ci_variables repository_ci_variables_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_ci_variables
    ADD CONSTRAINT repository_ci_variables_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: repository_collaborators repository_collaborators_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_collaborators
    ADD CONSTRAINT repository_collaborators_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: repository_collaborators repository_collaborators_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_collaborators
    ADD CONSTRAINT repository_collaborators_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: repository_settings repository_settings_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_settings
    ADD CONSTRAINT repository_settings_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: repository_stars repository_stars_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_stars
    ADD CONSTRAINT repository_stars_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: repository_stars repository_stars_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.repository_stars
    ADD CONSTRAINT repository_stars_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: webhook_deliveries webhook_deliveries_webhook_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhook_deliveries
    ADD CONSTRAINT webhook_deliveries_webhook_id_fkey FOREIGN KEY (webhook_id) REFERENCES public.webhooks(id) ON DELETE CASCADE;


--
-- Name: webhooks webhooks_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webhooks
    ADD CONSTRAINT webhooks_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- Name: wikis wikis_repository_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.wikis
    ADD CONSTRAINT wikis_repository_id_fkey FOREIGN KEY (repository_id) REFERENCES public.repositories(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

-- ============================================================================
-- 0002_metrics_snapshots
-- ============================================================================

-- migrations/0002_metrics_snapshots.sql
--
-- One row per periodic metrics snapshot (written on an hourly timer from
-- ferrisgit-api's main.rs), insert-only, so the admin dashboard can chart the
-- evolution of otherwise current-value-only totals over time.
CREATE TABLE metrics_snapshots (
    id UUID PRIMARY KEY,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    total_users BIGINT NOT NULL,
    total_repositories BIGINT NOT NULL,
    total_storage_bytes BIGINT NOT NULL
);
CREATE INDEX metrics_snapshots_recorded_at_idx ON metrics_snapshots (recorded_at);

-- ============================================================================
-- 0003_run_timestamps
-- ============================================================================

ALTER TABLE jobs ADD COLUMN started_at timestamptz;
ALTER TABLE jobs ADD COLUMN finished_at timestamptz;
ALTER TABLE pipelines ADD COLUMN finished_at timestamptz;

-- ============================================================================
-- 0004_merge_request_events
-- ============================================================================

CREATE TABLE merge_request_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    merge_request_id uuid NOT NULL REFERENCES merge_requests(id) ON DELETE CASCADE,
    actor_id uuid REFERENCES users(id) ON DELETE SET NULL,
    kind text NOT NULL CHECK (kind IN ('review_submitted','labels_changed','milestone_changed','title_changed','commits_pushed','merged','closed','thread_resolved','thread_reopened')),
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX merge_request_events_mr_idx ON merge_request_events (merge_request_id, created_at);

ALTER TABLE merge_requests ADD COLUMN head_sha text;

-- ============================================================================
-- 0005_smtp_settings
-- ============================================================================

-- Single-row SMTP configuration (id is pinned to true, like system_settings). The password is stored
-- encrypted (AES-GCM, SETTINGS_ENCRYPTION_KEY); NULL when the relay needs no authentication.
CREATE TABLE smtp_settings (
    id boolean PRIMARY KEY DEFAULT true CHECK (id = true),
    host text NOT NULL,
    port integer NOT NULL CHECK (port BETWEEN 1 AND 65535),
    security text NOT NULL CHECK (security IN ('none', 'starttls', 'tls')),
    username text NOT NULL DEFAULT '',
    encrypted_password bytea,
    from_address text NOT NULL,
    from_name text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================================
-- 0006_mfa
-- ============================================================================

-- Mandatory MFA (lot 2): one TOTP credential per user (secret encrypted at rest with SETTINGS_ENCRYPTION_KEY;
-- `confirmed = false` while the user is still enrolling) and single-use backup codes stored as salted hashes.
CREATE TABLE totp_credentials (
    user_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    encrypted_secret bytea NOT NULL,
    confirmed boolean NOT NULL DEFAULT false,
    last_used_step bigint,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE mfa_backup_codes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash text NOT NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX mfa_backup_codes_user_idx ON mfa_backup_codes (user_id) WHERE used_at IS NULL;

-- ============================================================================
-- 0007_registration
-- ============================================================================

-- Account creation (lot 3): the admin switch for free self-registration (off by default) and the pending
-- activation links of invited users. Only the SHA-256 of a token is stored; one live invitation per user.
ALTER TABLE system_settings ADD COLUMN registration_enabled boolean NOT NULL DEFAULT false;

CREATE TABLE user_invitations (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================================
-- 0008_webauthn
-- ============================================================================

-- Passkeys (lot 4): WebAuthn credentials as an MFA factor, several per user. `passkey` is the serialized
-- webauthn-rs `Passkey` (public key, signature counter, backup flags): public material, so no encryption at rest.
-- `credential_id` is unique across ALL users: the same authenticator credential can never be attached twice.
CREATE TABLE webauthn_credentials (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name text NOT NULL,
    credential_id bytea NOT NULL UNIQUE,
    passkey jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz
);
CREATE INDEX webauthn_credentials_user_idx ON webauthn_credentials (user_id);

-- ============================================================================
-- 0009_password_reset
-- ============================================================================

-- Admin-triggered password reset: the pending reset links of existing accounts, same shape as `user_invitations`.
-- Only the SHA-256 of a token is stored. One live link per user (`user_id` UNIQUE): a second admin reset replaces
-- the first link instead of leaving several valid ones around. `token_hash` UNIQUE is the lookup index of `consume`.
-- The admin promotion/demotion needs no schema change: `users.is_admin` already exists.
CREATE TABLE password_reset_tokens (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================================
-- 0010_anonymize_deleted_user_content
-- ============================================================================

-- Admin deletion of a user account. What the user OWNS goes with them (their personal repositories are deleted by the
-- application beforehand, then the `ON DELETE CASCADE` foreign keys of 0001 take their tokens, memberships, factors,
-- issues, ...). What they WROTE in places they do not own stays, attributed to nobody: a group they created, a merge
-- request they opened or a comment they left on someone else's repository, a release or an asset they published.
-- Those five author references were `NOT NULL` with a plain (RESTRICT) foreign key, which would refuse the deletion
-- outright; they become nullable and `ON DELETE SET NULL`, like `issues.assignee_id` already is. A `NULL` author reads
-- as a deleted user everywhere these rows are shown.

ALTER TABLE groups ALTER COLUMN created_by DROP NOT NULL;
ALTER TABLE groups DROP CONSTRAINT groups_created_by_fkey;
ALTER TABLE groups ADD CONSTRAINT groups_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE merge_request_comments ALTER COLUMN author_id DROP NOT NULL;
ALTER TABLE merge_request_comments DROP CONSTRAINT merge_request_comments_author_id_fkey;
ALTER TABLE merge_request_comments ADD CONSTRAINT merge_request_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE merge_requests ALTER COLUMN author_id DROP NOT NULL;
ALTER TABLE merge_requests DROP CONSTRAINT merge_requests_author_id_fkey;
ALTER TABLE merge_requests ADD CONSTRAINT merge_requests_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE release_assets ALTER COLUMN uploaded_by DROP NOT NULL;
ALTER TABLE release_assets DROP CONSTRAINT release_assets_uploaded_by_fkey;
ALTER TABLE release_assets ADD CONSTRAINT release_assets_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES public.users(id) ON DELETE SET NULL;

ALTER TABLE releases ALTER COLUMN author_id DROP NOT NULL;
ALTER TABLE releases DROP CONSTRAINT releases_author_id_fkey;
ALTER TABLE releases ADD CONSTRAINT releases_author_id_fkey FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE SET NULL;
