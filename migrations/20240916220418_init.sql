CREATE TABLE logs
(
    id        BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    msg_id    INT      NOT NULL,
    user_id   BIGINT      NOT NULL,
    chat_id   BIGINT      NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX logs_chat_msg_id_idx ON logs (chat_id, msg_id);
CREATE INDEX logs_user_timestamp_idx ON logs (user_id, timestamp);
CREATE INDEX logs_chat_user_idx ON logs (chat_id, user_id);