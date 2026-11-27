-- Usernames are free-form until the auth module adds its own rules.
ALTER TABLE users DROP CONSTRAINT users_username_check;
