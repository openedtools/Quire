-- The master switch for sending data to cloud services (spec F-MOD-5, plan PC2). No row means
-- the decision has not been made yet: the first launch of this build makes it once.
CREATE TABLE IF NOT EXISTS cloud_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    allow_cloud INTEGER NOT NULL DEFAULT 0,
    -- What an upgrade found enabled, shown once in a notice; NULL when there is nothing to say
    notice TEXT
);
