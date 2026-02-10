CREATE TABLE mail_busy_labels (
    id INTEGER NOT NULL PRIMARY KEY,

    CONSTRAINT mail_busy_labels_id
    FOREIGN KEY (id)
    REFERENCES labels (local_id)
    ON DELETE CASCADE
);
