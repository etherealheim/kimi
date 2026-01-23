# Privacy & Local Data Summary

This document summarizes what stays on your machine and what's shared via git.

## 🔒 Local Only (Never Committed)

These files/directories are **excluded from git** and stay private on your machine:

| Path | Contains | Why Local |
|------|----------|-----------|
| `data/kimi.db/` | Conversation history, embeddings | Your private chat data |
| `data/personalities/My personality.md` | Your personal profile | Name, location, preferences |
| `config.local.toml` | API keys, vault path | Sensitive credentials |
| `config.local.toml` (if exists) | Legacy database | Old chat history |

## ✅ Shared via Git (Safe to Commit)

These files are **tracked in git** and shared with all users:

| Path | Contains | Why Shared |
|------|----------|------------|
| `config.toml` | Default settings, **empty API keys** | Template configuration |
| `config.local.toml.example` | Template for local config | Helps new users set up |
| `data/personalities/*.md` (except My personality.md) | AI personalities | Shared character templates |
| `data/personalities/My personality.template.md` | Empty profile template | Helps new users create profile |

## 🛡️ Security Guarantees

### API Keys
- ✅ Stored in `config.local.toml` (gitignored)
- ✅ Auto-stripped from `config.toml` when app saves
- ✅ Never appear in git history (verified)
- ❌ Never committed, never pushed

### Personal Data
- ✅ Chat history stays in local database
- ✅ Personal profile excluded from git
- ✅ Vault path in local config only
- ❌ Never leaves your machine

### What Gets Shared
- ✅ Empty templates
- ✅ Default settings
- ✅ AI personality definitions (Kimi, Casca, etc.)
- ✅ Documentation

## 📋 Setup Checklist for New Users

When someone clones the repo, they get:

1. ✅ Empty database (auto-created on first run)
2. ✅ Template config with no API keys
3. ✅ Empty personal profile template
4. ✅ Default AI personalities

They need to:

1. `cp config.local.toml.example config.local.toml`
2. Edit `config.local.toml` with their API keys (optional)
3. Edit `data/personalities/My personality.md` with their info (auto-created from template)
4. Run the app

## 🔍 Verification Commands

### Check .gitignore Rules
```bash
git check-ignore -v config.local.toml data/kimi.db/
# Should show both are ignored
```

### Check No Keys in History
```bash
git log -p config.toml | grep -i "api_key.*=.*[A-Za-z0-9]"
# Should return nothing (only empty strings)
```

### Check Local Files Exist
```bash
ls -la config.local.toml data/personalities/"My personality.md"
# Should show your local files
```

### Check What's Tracked
```bash
git ls-files | grep -E "(config|personality)"
# Should NOT show config.local.toml or My personality.md
```

## 🚨 If You Accidentally Commit Secrets

### Step 1: Remove from Staging (Before Push)
```bash
git reset HEAD config.local.toml
git checkout config.local.toml
```

### Step 2: Remove from History (After Push)
```bash
# WARNING: Rewrites history - coordinate with team
git filter-branch --force --index-filter \
  'git rm --cached --ignore-unmatch config.local.toml' \
  --prune-empty --tag-name-filter cat -- --all

# Force push
git push origin --force --all
```

### Step 3: Regenerate Compromised Keys
- **ElevenLabs**: Regenerate at https://elevenlabs.io/
- **Venice AI**: Regenerate at https://venice.ai/
- **Gab AI**: Regenerate at https://gab.ai/
- **Brave Search**: Regenerate at https://brave.com/search/api/

## 📚 Related Documentation

- [CONFIG.md](CONFIG.md) - Configuration system details
- [DATABASE.md](DATABASE.md) - Database management
- [README.md](README.md) - Getting started guide

---

**Summary**: Your API keys, chat history, and personal profile are protected by `.gitignore` and designed to stay local. The repository only contains safe templates and defaults.
