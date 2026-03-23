CREATE TABLE IF NOT EXISTS sources (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    platform   TEXT    NOT NULL,
    username   TEXT    NOT NULL,
    ai_cutoff_date TEXT,
    created_at TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(platform, username)
);

CREATE TABLE IF NOT EXISTS posts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    external_id TEXT    NOT NULL,
    post_type   TEXT    NOT NULL,
    body        TEXT    NOT NULL,
    url         TEXT,
    repo        TEXT,
    created_at  TEXT    NOT NULL,
    likely_ai   INTEGER NOT NULL DEFAULT 0,
    scraped_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, external_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
    body,
    post_type,
    repo,
    content='posts',
    content_rowid='id'
);

-- Triggers to keep FTS index in sync with posts table
CREATE TRIGGER IF NOT EXISTS posts_ai AFTER INSERT ON posts BEGIN
    INSERT INTO posts_fts(rowid, body, post_type, repo)
    VALUES (new.id, new.body, new.post_type, new.repo);
END;

CREATE TRIGGER IF NOT EXISTS posts_ad AFTER DELETE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, body, post_type, repo)
    VALUES ('delete', old.id, old.body, old.post_type, old.repo);
END;

CREATE TRIGGER IF NOT EXISTS posts_au AFTER UPDATE ON posts BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, body, post_type, repo)
    VALUES ('delete', old.id, old.body, old.post_type, old.repo);
    INSERT INTO posts_fts(rowid, body, post_type, repo)
    VALUES (new.id, new.body, new.post_type, new.repo);
END;

CREATE TABLE IF NOT EXISTS scrape_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    started_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    finished_at   TEXT,
    posts_fetched INTEGER NOT NULL DEFAULT 0,
    cursor        TEXT
);
