# Configuration System

Kimi uses a two-file configuration system to keep your API keys private while sharing safe defaults.

## Configuration Files

### `config.toml` (Tracked in Git)
- **Shared across all users**
- Contains default settings and model configurations
- **API keys are EMPTY** (template only)
- Safe to commit and push to GitHub

### `config.local.toml` (Local Only - Gitignored)
- **Your personal configuration**
- Contains your actual API keys and personal paths
- **Never committed to git** (protected by .gitignore)
- Overrides values from `config.toml`

## How It Works

1. **Loading Priority**:
   ```
   config.toml (defaults) → config.local.toml (overrides) → Final Config
   ```

2. **When you save settings** (via the app):
   - API keys are **automatically stripped** from `config.toml`
   - Your keys remain safe in `config.local.toml`
   - See `redacted_for_project()` in `src/config.rs:288`

3. **Selective Overrides**:
   - You only need to include settings you want to override
   - Missing values default to `config.toml`

## Setup for New Users

### 1. Copy the Template
```bash
cp config.local.toml.example config.local.toml
```

### 2. Add Your API Keys
Edit `config.local.toml` with your actual credentials:

```toml
[elevenlabs]
api_key = "your_actual_key_here"

[venice]
api_key = "your_actual_key_here"

[gab]
api_key = "your_actual_key_here"

[brave]
api_key = "your_actual_key_here"

[obsidian]
vault_path = "/path/to/your/vault/"
```

### 3. Run the App
```bash
cargo run
```

The app automatically:
- Loads defaults from `config.toml`
- Overlays your secrets from `config.local.toml`
- You're ready to go!

## API Keys Reference

### ElevenLabs (Text-to-Speech)
- **Get Key**: https://elevenlabs.io/
- **Used for**: Voice synthesis (Ctrl+R in chat)
- **Optional**: App works without TTS

### Venice AI
- **Get Key**: https://venice.ai/
- **Used for**: Alternative AI backend
- **Optional**: Can use Ollama instead

### Gab AI
- **Get Key**: https://gab.ai/
- **Used for**: Alternative AI backend
- **Optional**: Can use Ollama instead

### Brave Search
- **Get Key**: https://brave.com/search/api/
- **Used for**: Web search integration
- **Optional**: App works without search

### Obsidian Vault
- **Path**: Local filesystem path to your Obsidian vault
- **Used for**: Note context integration
- **Optional**: App works without Obsidian

## Security Best Practices

### ✅ DO:
- Keep API keys in `config.local.toml` only
- Use `.gitignore` to protect local config (already configured)
- Share `config.local.toml.example` as a template
- Commit changes to `config.toml` (defaults only)

### ❌ DON'T:
- Never put real API keys in `config.toml`
- Never commit `config.local.toml` to git
- Never share your `config.local.toml` file
- Never disable the `.gitignore` rule for this file

## Verification

### Check if API Keys are Protected
```bash
# Should return empty strings:
grep "api_key" config.toml

# Should be listed (proving it's ignored):
git check-ignore -v config.local.toml
```

### Check Git History for Leaked Keys
```bash
# Should show no sensitive data:
git log -p config.toml | grep -i "api_key"
```

## File Structure

```
kimi/
├── config.toml                    # ✅ Tracked (empty keys)
├── config.local.toml              # ❌ NOT tracked (your keys)
├── config.local.toml.example      # ✅ Tracked (template)
└── .gitignore                     # Contains: config.local.toml
```

## Troubleshooting

### "API Key Invalid" Errors
1. Check `config.local.toml` exists
2. Verify API key format (no extra spaces/quotes)
3. Restart the app to reload config

### Config Not Loading
1. Ensure `config.local.toml` is in the project root
2. Check TOML syntax is valid: `cargo run` will show parse errors
3. Verify file permissions are readable

### Keys Accidentally Committed
```bash
# Remove from git history (nuclear option):
git filter-branch --force --index-filter \
  'git rm --cached --ignore-unmatch config.local.toml' \
  --prune-empty --tag-name-filter cat -- --all

# Regenerate compromised API keys immediately!
```

## Advanced: Config Override Order

The system checks locations in this order:

1. **Project config**: `./config.toml` (current directory)
2. **Legacy config**: `~/.config/kimi/config.toml` (if exists, migrated automatically)
3. **Local overrides**: `./config.local.toml` (merged on top)
4. **Defaults**: Hardcoded in `src/config.rs` (fallback)

See `Config::load()` in `src/config.rs:192` for implementation details.

## Example: Minimal config.local.toml

If you only use Ollama (local models) and Obsidian:

```toml
[obsidian]
vault_path = "/home/user/my-vault/"

# That's it! No API keys needed.
```

All other services will use empty keys (disabled) or defaults.

---

**Privacy Note**: Your API keys never leave your machine and are automatically excluded from git commits. The two-file system ensures your secrets stay private while allowing you to collaborate safely.
