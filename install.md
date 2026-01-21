# Install

## Prerequisites
- Rust toolchain (stable). Install via `rustup`: https://rustup.rs
- Ollama installed and running: https://ollama.com

## Build and run
- `cargo build`
- `cargo run`

## Ollama setup
- Start Ollama: `ollama serve`
- Pull the default chat model: `ollama pull gemma3:12b`
- Optional: pull the translation model: `ollama pull translategemma:latest`
- Configure the endpoint in `config.toml` (`ollama.url`), default is `http://localhost:11434`

## API keys
Keys are loaded from a local override file so they do not go into git.

1. Create `config.local.toml` in the project root:
```
[elevenlabs]
api_key = "your_key"

[venice]
api_key = "your_key"

[brave]
api_key = "your_key"
```
2. Keep non-secret defaults in `config.toml` (it is saved with keys redacted).

## Memories
- Stored at `data/personalities/Memories.md`
- Edit via the Personality menu → `Memories`

## Personalities
- Stored at `data/personalities/`
- `My personality.md` is the base user profile
- Create/edit via the Personality menu

## Shortcuts
- `Ctrl+C` quit
- `/` command menu (in chat/history)
- `Tab` rotate agent CHAT/TRANSLATE
- `Ctrl+R` speak last response
- `Ctrl+T` toggle auto-TTS
- `Ctrl+P` toggle personality
- `Esc` back/close

## Commands
convert [type] path-to-file
download [link-to-instagram-facebook-twitter-youtube]
today returns todays date
weather returns weather
time returns time