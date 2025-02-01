# William

William is a work-in-progress chat client that stores everything locally to your machine. API keys are completely local,
and only exposed to their respective LLM providers.

# Features

- [x] Markdown chats
- [x] o1 models
- [x] RAG memory
- [ ] Secure API key storage (is this even feasible locally?)
- [ ] (Better) Conversation forking + message retries
- [ ] Artifacts
- [ ] Non-webkit UI
- [ ] Actual documentation
- [ ] Better cross-platform support/testing (this has really only been tested on MacOS)
- [ ] Non-text media usage
- [ ] On-screen modal/pop-up chat desktop integration chat (what is this called?)

# Usage

There are currently no pre-built binaries.

To build locally, you'll first need to install [Tauri Prerequisites](https://tauri.app/start/prerequisites/).
Then,
```sh
npm run tauri dev # for local development
npm run tauri build # for a production build
```
