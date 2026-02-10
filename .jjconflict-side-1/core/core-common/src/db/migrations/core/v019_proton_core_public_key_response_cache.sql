CREATE TABLE public_address_key_response_cache
(
    email         TEXT    NOT NULL,
    internal_only INTEGER NOT NULL,
    response      TEXT    NOT NULL,

    PRIMARY KEY (email, internal_only)
);