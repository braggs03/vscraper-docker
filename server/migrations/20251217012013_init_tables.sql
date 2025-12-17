-- Add migration script here
CREATE TABLE IF NOT EXISTS
    Config (
        id INTEGER PRIMARY KEY NOT NULL,
        skip_homepage BOOLEAN NOT NULL
    );

CREATE TABLE IF NOT EXISTS
    Download (
        id INTEGER PRIMARY KEY NOT NULL,
        status TEXT NOT NULL
    );