#### About

Kimi is an experimental Rust TUI chat assistant. `Vibe-coded`, but with good intentions.

<img width="400" height="380" alt="image" src="https://github.com/user-attachments/assets/65fa3b59-baac-4abe-8ac0-373ed4b52e9b" /> <img width="400" height="380" alt="image" src="https://github.com/user-attachments/assets/e9a1f625-dd59-44a0-a412-3ac57008e2db" />

#### Upcoming Features

- [ ] Evolving Personality System - An adaptive personality system that learns and evolves based on user interactions

#### Quick Start
1. Install Rust by following the instructions at https://www.rust-lang.org/tools/install.

2. Install Ollama by following the official guidance at https://ollama.com/download.

3. Using Ollama, download the required models:
   - Run: `ollama pull gemma3:12b` - Choose any model you like
   - Run: `ollama pull functiongemma:latest` - Required for deciding intentions
   - Run: `ollama pull translategemma:latest` - non-mandatory, specialized

4. Obtain API keys for ElevenLabs, Venice, Gab, and Brave Search as needed.

5. Rename a file named `config.local.toml.example` to `config.local.toml` in the project root and enter your API keys

6. Define your personality by editing the file at `data/personalities/My personality.md` from `data/personalities/My personality.template.md`.


The app works out of the box with Ollama (local models). API keys for ElevenLabs, Venice, Gab, and Brave Search are optional.

#### Database

The application uses SurrealDB (RocksDB backend) for conversation history and embeddings. The database:

- **Location**: `./data/kimi.db/`
- **Not tracked in git**: Your conversation history stays private and local

#### FAQ

1. Some models are too assertive and dismiss the Identity.Choose more submissive model