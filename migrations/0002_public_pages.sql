-- Anonymous read-only pages for public repositories (on by default) and whether search engines may index them
-- (off by default).
ALTER TABLE system_settings
    ADD COLUMN public_pages_enabled boolean NOT NULL DEFAULT true,
    ADD COLUMN seo_indexing_enabled boolean NOT NULL DEFAULT false;
