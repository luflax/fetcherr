-- Requests: a title and the part of it wanted (scope), with its state.
CREATE TABLE requests (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT    NOT NULL CHECK (kind IN ('movie', 'series')),
    tmdb_id     INTEGER NOT NULL,
    title       TEXT    NOT NULL,
    year        INTEGER,
    poster_path TEXT,
    -- JSON `Scope`; `scope_key` is its canonical text, for uniqueness.
    scope       TEXT    NOT NULL,
    scope_key   TEXT    NOT NULL,
    -- Quality id that overrides the media type's ranking.
    quality     TEXT,
    -- Release picked by hand, used by the first search instead of picking one.
    release_id  TEXT,
    state       TEXT    NOT NULL,
    error       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (kind, tmdb_id, scope_key)
);
CREATE INDEX requests_by_state ON requests (state);
CREATE INDEX requests_by_title ON requests (kind, tmdb_id);

-- The torrents grabbed for a request.
CREATE TABLE request_torrents (
    request_id    INTEGER NOT NULL REFERENCES requests (id) ON DELETE CASCADE,
    info_hash     TEXT    NOT NULL,
    release_title TEXT    NOT NULL,
    quality       TEXT    NOT NULL,
    size          INTEGER NOT NULL,
    -- JSON `Coverage`: what the release contains.
    coverage      TEXT    NOT NULL,
    state         TEXT    NOT NULL,
    error         TEXT,
    added_at      INTEGER NOT NULL,
    PRIMARY KEY (request_id, info_hash)
);
CREATE INDEX request_torrents_by_state ON request_torrents (state);

-- Files placed in the library.
CREATE TABLE imported_files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id  INTEGER NOT NULL REFERENCES requests (id) ON DELETE CASCADE,
    info_hash   TEXT    NOT NULL,
    source      TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    season      INTEGER,
    episode     INTEGER,
    imported_at INTEGER NOT NULL
);
CREATE INDEX imported_files_by_request ON imported_files (request_id);

-- OMDb answers, kept across restarts (OMDb allows 1000 requests a day).
CREATE TABLE ratings_cache (
    imdb_id    TEXT    PRIMARY KEY,
    payload    TEXT    NOT NULL,
    fetched_at INTEGER NOT NULL
);
