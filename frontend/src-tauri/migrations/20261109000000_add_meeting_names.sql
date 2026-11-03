-- Automatic meeting names (plan Phase L, PL1 and PL2). No row means the defaults: on, with the
-- format "{date} {topic}".
CREATE TABLE IF NOT EXISTS naming_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    auto_name INTEGER NOT NULL DEFAULT 1,
    title_format TEXT NOT NULL DEFAULT '{date} {topic}'
);
