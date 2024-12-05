CREATE TABLE IF NOT EXISTS message_types (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

INSERT INTO message_types (name) 
SELECT 'system' WHERE NOT EXISTS (SELECT 1 FROM message_types);

INSERT INTO message_types (name) 
SELECT 'user' WHERE NOT EXISTS (SELECT 1 FROM message_types);

INSERT INTO message_types (name) 
SELECT 'assistant' WHERE NOT EXISTS (SELECT 1 FROM message_types);

CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    message_type_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    model TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    FOREIGN KEY (message_type_id) REFERENCES message_types(id)
);

CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id)
);
