CREATE TABLE sender_image_cache (
    address TEXT NOT NULL,
    bimi_selector TEXT DEFAULT NULL,
    format TEXT DEFAULT NULL,
    mode INTEGER DEFAULT NULL,
    size INTEGER DEFAULT NULL,

    path TEXT
);
