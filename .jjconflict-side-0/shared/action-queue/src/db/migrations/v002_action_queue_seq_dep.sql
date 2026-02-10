ALTER TABLE action_queue_dependencies
    ADD COLUMN
        dependency_type INTEGER NOT NULL DEFAULT 0;
