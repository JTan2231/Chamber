CREATE TABLE IF NOT EXISTS message_types (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

INSERT INTO message_types (name)
SELECT 'system'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'system');

INSERT INTO message_types (name)
SELECT 'user'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'user');

INSERT INTO message_types (name)
SELECT 'assistant'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'assistant');

CREATE TABLE IF NOT EXISTS providers (
    name TEXT PRIMARY KEY
);

INSERT INTO providers (name)
SELECT 'openai'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'openai');

INSERT INTO providers (name)
SELECT 'groq'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'groq');

INSERT INTO providers (name)
SELECT 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'anthropic');

CREATE TABLE IF NOT EXISTS models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT,
    provider TEXT NOT NULL,
    FOREIGN KEY (provider) REFERENCES providers(name)
);

INSERT INTO models (name, provider)
SELECT 'gpt-4o', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'gpt-4o' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'gpt-4o-mini', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'gpt-4o-mini' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'o1-preview', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'o1-preview' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'o1-mini', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'o1-mini' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'llama3-70b-8192', 'groq'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'llama3-70b-8192' AND provider = 'groq');

INSERT INTO models (name, provider)
SELECT 'claude-3-opus-20240229', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-opus-20240229' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-sonnet-20240229', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-sonnet-20240229' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-haiku-20240307', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-haiku-20240307' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-5-sonnet-latest', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-5-sonnet-latest' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-5-haiku-latest', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-5-haiku-latest' AND provider = 'anthropic');

CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    message_type_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    api_config_id INTEGER NOT NULL,
    system_prompt TEXT NOT NULL,
    FOREIGN KEY (message_type_id) REFERENCES message_types(id),
    FOREIGN KEY (api_config_id) REFERENCES api_configurations(id)
);

CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id)
);
