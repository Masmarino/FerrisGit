-- The language an account chose for the interface. NULL until it chooses one: the interface then follows the browser.
ALTER TABLE users
    ADD COLUMN language text
        CHECK (language IN ('fr', 'en', 'es', 'it', 'de'));
