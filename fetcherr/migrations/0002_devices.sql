-- Devices paired with a code (TVs, phones). Each has its own API key, stored as a SHA-256 hash.
CREATE TABLE devices (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT    NOT NULL,
    key_hash     TEXT    NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER
);
