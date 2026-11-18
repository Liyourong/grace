CREATE TABLE paper_folders (
    id BIGSERIAL PRIMARY KEY,
    owner_id BIGINT NOT NULL REFERENCES users ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    UNIQUE (owner_id, id)
);
CREATE UNIQUE INDEX paper_folder_names ON paper_folders (owner_id, lower(name));

ALTER TABLE papers ADD COLUMN folder_id BIGINT;
ALTER TABLE papers ADD CONSTRAINT papers_folder_owner FOREIGN KEY (owner_id, folder_id)
    REFERENCES paper_folders (owner_id, id) ON DELETE SET NULL (folder_id);

-- A delivery survives paper deletion. Slot 1 is the original French Basics.
CREATE TABLE starter_deliveries (
    owner_id BIGINT NOT NULL REFERENCES users ON DELETE CASCADE,
    slot SMALLINT NOT NULL,
    paper_id BIGINT UNIQUE REFERENCES papers ON DELETE SET NULL,
    PRIMARY KEY (owner_id, slot)
);
INSERT INTO starter_deliveries (owner_id, slot, paper_id)
SELECT owner_id, 1, id FROM papers WHERE is_default;
