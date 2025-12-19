-- Add migration script here
CREATE TABLE IF NOT EXISTS
    Config (
        id INTEGER PRIMARY KEY NOT NULL,
        skip_homepage BOOLEAN NOT NULL
    );

CREATE TABLE IF NOT EXISTS
    Download (
        url TEXT PRIMARY KEY NOT NULL, 
        status TEXT NOT NULL,
        container TEXT NOT NULL,
        name_format TEXT NOT NULL, 
        quality TEXT NOT NULL
    );