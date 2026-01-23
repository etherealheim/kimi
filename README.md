Kimi is an experimental Rust TUI chat assistant.

## Quick Start

1. **Clone the repository**
   ```bash
   git clone <repo-url>
   cd kimi
   ```

2. **Set up local configuration** (optional - for API services)
   ```bash
   cp config.local.toml.example config.local.toml
   # Edit config.local.toml with your API keys
   ```

3. **Run the app**
   ```bash
   cargo run
   ```

The app works out of the box with Ollama (local models). API keys for ElevenLabs, Venice, Gab, and Brave Search are optional.

See [CONFIG.md](CONFIG.md) for detailed configuration documentation.

## Database

The application uses SurrealDB (RocksDB backend) for conversation history and embeddings. The database:

- **Location**: `./data/kimi.db/` (local to your clone)
- **Auto-initializes**: Database and schema are created automatically on first run
- **Not tracked in git**: Your conversation history stays private and local
- **Fresh on clone**: Each new clone gets a clean database

No setup required - just run the app and the database will be ready.

## Upcoming Features

[ ] Evolving Personality System - An adaptive personality system that learns and evolves based on user interactions