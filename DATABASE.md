# Database Setup & Management

## Overview

Kimi uses **SurrealDB** with a RocksDB backend for persistent storage of:
- Conversation history
- Message embeddings (for semantic search)
- Conversation summaries

## Database Location

```
./data/kimi.db/    # Local RocksDB files (not tracked in git)
```

## Automatic Initialization

**No manual setup required.** The database automatically:

1. Creates the `data/` directory if it doesn't exist
2. Initializes the RocksDB database at `data/kimi.db/`
3. Creates the schema (tables, fields, indexes)
4. Sets up embedding indexes for vector search

This happens on first run in `StorageManager::new()` (see `src/storage.rs:134-146`).

## Git Tracking

### What's Tracked
- `data/personalities/*.md` - Personality definition files (shared across users)
- `data/personalities/My personality.template.md` - Template for personal profile

### What's NOT Tracked (Local Only)
- `data/kimi.db/` - Your personal conversation database
- `data/history.db` - Legacy database (deprecated)
- `data/personalities/My personality.md` - Your personal profile
- `config.local.toml` - Local configuration with API keys

### Configuration

`.gitignore` rules:
```gitignore
data/kimi.db/                           # Main database directory
data/history.db                          # Legacy database file
data/personalities/My personality.md     # Personal profile (local only)
config.local.toml                        # Local config with API keys
```

**Related**: See [CONFIG.md](CONFIG.md) for configuration system documentation.

### Personal Profile Setup

**"My personality.md"** contains your personal information (name, preferences, context tags).

- ✅ **Template tracked**: `My personality.template.md` is in git
- ❌ **Your data NOT tracked**: `My personality.md` is local-only
- 🔄 **Auto-created**: If missing, created from template on first run
- 🔒 **Private**: Your personal info never leaves your machine

## For New Contributors

When you clone the repository:

1. ✅ `data/personalities/` directory exists (tracked in git)
2. ✅ Database will auto-initialize on first run
3. ✅ You get a fresh, empty conversation history
4. ✅ No additional setup required

## Database Schema

### Tables

**`conversation`**
- `agent_name`: string - Which agent was used
- `summary`: optional string - Brief conversation summary
- `detailed_summary`: optional string - Extended summary
- `created_at`: string - ISO timestamp
- `updated_at`: string - ISO timestamp

**`message`**
- `conversation`: relation - Links to conversation table
- `role`: string - "User" | "Assistant" | "System"
- `content`: string - Message text
- `embedding`: optional array - 384-dim vector (for semantic search)
- `timestamp`: string - Message timestamp
- `display_name`: optional string - Custom display name

### Indexes

- **Vector search index**: `mteb_embedding_index` on `message.embedding`
  - Enables semantic similarity search
  - 384 dimensions (matches all-MiniLM-L6-v2 model)
  - Cosine distance metric

## Backup & Migration

### Backup Your Database

```bash
# Copy the entire database directory
cp -r data/kimi.db data/kimi.db.backup-$(date +%Y%m%d)
```

### Reset Database

```bash
# Delete database (will auto-recreate on next run)
rm -rf data/kimi.db/
```

### Export Conversations (Future Feature)

Not yet implemented. Tracked in `my-to-do.md`.

## Troubleshooting

### Database Won't Initialize

Check:
1. Write permissions in `./data/` directory
2. Disk space available
3. RocksDB files aren't corrupted

### Database Corruption

```bash
# Nuclear option: delete and reinitialize
rm -rf data/kimi.db/
cargo run  # Will auto-recreate
```

### Migration from Old Version

If you have `data/history.db` (old SQLite database):
- Migration not yet implemented
- Old history won't be automatically imported
- Keep the old file if you need to manually export data

## Privacy Note

⚠️ Your conversation history is **local only** and never leaves your machine unless you explicitly share or back it up.

The database is excluded from git commits, so your conversations won't accidentally be pushed to GitHub.
