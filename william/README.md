# William

William is a work-in-progress chat client that stores everything locally to your machine. API keys are completely local,
and only exposed to their respective LLM providers.

# Features

[x] Markdown chats
[x] o1 models
[x] RAG memory
[ ] Secure API key storage
[ ] (Better) Conversation forking + message retries
[ ] Artifacts
[ ] Non-webkit UI

# Usage

There are currently no pre-built binaries.

To build locally, you'll first need to install [Tauri Prerequisites](https://tauri.app/start/prerequisites/).
Then,
```sh
npm run tauri dev # for local development
npm run tauri build # for a production build
```
