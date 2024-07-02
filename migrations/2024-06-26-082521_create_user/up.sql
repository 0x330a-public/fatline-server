-- Your SQL goes here
CREATE TABLE IF NOT EXISTS users (
    fid bigint primary key,
    username text,
    display_name text,
    bio text,
    url text,
    profile_pic text
);

CREATE TABLE IF NOT EXISTS signers (
    pk bytea PRIMARY KEY,
    fid bigint REFERENCES users ON DELETE CASCADE NOT NULL,
    active boolean NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS links (
    fid bigint,
    target bigint,
    timestamp timestamp,
    primary key (fid, target)
);
