CREATE TABLE busy_labels (
    id INTEGER NOT NULL PRIMARY KEY,

    CONSTRAINT busy_labels_id
    FOREIGN KEY (id)
    REFERENCES labels (local_id)
    ON DELETE CASCADE
);
