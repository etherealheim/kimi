Kimi is an experimental Rust TUI chat assistant.

## Database

The application uses SurrealDB (RocksDB backend) for conversation history and embeddings. The database:

- **Location**: `./data/kimi.db/` (local to your clone)
- **Auto-initializes**: Database and schema are created automatically on first run
- **Not tracked in git**: Your conversation history stays private and local
- **Fresh on clone**: Each new clone gets a clean database

No setup required - just run the app and the database will be ready.

## Upcoming Features

[ ] Evolving Personality System - An adaptive personality system that learns and evolves based on user interactions