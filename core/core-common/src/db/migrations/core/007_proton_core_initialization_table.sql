-- A table that stores information about which component/service/provider is initialized and ready to work.
-- It prevents us from double-initialization, as well as informs when the application is ready for user interactions or events from the network.
-- If the entry exists, it means it has been initialized

CREATE TABLE initialized_components (
    key TEXT NOT NULL PRIMARY KEY,
    state INTEGER NOT NULL DEFAULT 0
);
