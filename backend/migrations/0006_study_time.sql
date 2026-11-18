-- Adjacent and overlapping intervals collapse into one range, including concurrent reports.
CREATE TABLE study_time (
    user_id BIGINT NOT NULL REFERENCES users ON DELETE CASCADE,
    utc_day BIGINT NOT NULL,
    intervals INT8MULTIRANGE NOT NULL,
    PRIMARY KEY (user_id, utc_day)
);
