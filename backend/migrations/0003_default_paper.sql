-- Every user gets one built-in paper; the partial unique index keeps it to one even under concurrent requests.
ALTER TABLE papers ADD COLUMN is_default BOOLEAN NOT NULL DEFAULT false;
CREATE UNIQUE INDEX papers_one_default ON papers (owner_id) WHERE is_default;
