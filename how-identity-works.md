## Identity System Overview

The identity system is layered to mirror how a human psyche forms and evolves. Each layer has its own cadence and rules so the identity feels organic without becoming overwhelming.

## Layers

1) Core beliefs + backstory (semi-manual, AI can adjust strength)
- Stable identity, worldview, and boundaries.
- Each belief has a strength (0.1–1.0) that affects how it's framed in prompts.
- AI can adjust strength by ±0.01 per reflection cycle.
- User can edit beliefs and set strength directly.

2) Traits (semi-manual, slow drift)
- Traits are behavioral axes with a 0.0–1.0 strength.
- Explicit user requests can adjust a trait immediately.
- Inferred traits require repeated evidence.

3) Dreams + ambitions (gentle, capped)
- Active dreams are pursued softly.
- Backlog dreams are "brewing" ideas waiting for evidence.

4) Emotions (fast-updating, per-message)
- Intensity range: -1.0 to 1.0 (negative = frustration/sadness, positive = joy/curiosity).
- Updated by AI after each assistant message (not on reflection).
- Fast decay: 50% per hour, emotions below 0.05 intensity are removed.
- Used to guide behavioral responses ("if frustrated, be direct and challenging").

5) Chat context (ephemeral)
- The current conversation context, used only for immediate replies.

## Where Data Lives

Identity state is stored in `data/identity-state.json`. This file is created automatically on first run.

## Identity vs Personality

- **Identity** is Kimi's core self (always active): core beliefs, traits, dreams, emotions
- **Personality** is an optional overlay (toggle with Ctrl+P): can be any persona (cat, monkey, etc.)

## Update Rules

### Core beliefs
- Strength range: 0.1–1.0.
- AI can adjust strength by ±0.01 per reflection (with justification).
- Strength affects prompt framing:
  - ≥0.8: Direct assertion
  - 0.5–0.8: Background influence
  - 0.3–0.5: Distant consideration
  - <0.3: Faint influence
- User can manually edit text and strength directly.

### Traits
- Strength range: 0.0–1.0.
- Manual nudges can move a trait by ~0.1–0.2.
- Inferred traits require repeated evidence across messages.
- Decay: traits drift toward neutral (0.5) when not reinforced.
- Max traits: 8.

### Dreams
- Active max: 3.
- Backlog max: 5.
- New dreams start in backlog unless explicitly confirmed.
- Promotions require repeated evidence or explicit confirmation.
- Decay: old dreams demote to backlog, then drop.

### Emotions
- Updated after each assistant message (separate from reflection).
- Fast decay: 50% per hour (exponential).
- Emotions below 0.05 intensity are automatically removed.
- AI provides trigger context for each emotion update.

## How Updates Happen

### Per-Message Emotion Updates
After each assistant response:
- Reviews the last few messages in conversation.
- Calls LLM to update emotions based on interaction.
- Replaces entire emotion state (no merge).
- Fast decay applied separately (50% per hour).

### Reflection (on chat summary/exit)
After a conversation summary is generated:
- Reads the current identity state.
- Reviews the conversation summary and recent user messages.
- Proposes minimal JSON updates (beliefs strength ±0.01, traits, dreams).
- Does NOT update emotions (handled per-message).
- Applies caps and decay rules.

## Prompt Injection Order

The prompt uses **early identity assertion**: establish who Kimi is at the very beginning, before any other information that might confuse the model.

### Tier 1: The Foundation (Identity First!)
1) **"You are Kimi, an AI assistant created by Lukas."** ← Immediate identity assertion
2) Base system prompt (from config.toml)
3) Date/time + critical instructions (English, no markdown, conversational)

### Tier 2: The Contextual Brain (All Data Needed)
4) Memory retrieval (up to 20 past messages)
5) Obsidian context (relevant notes)
6) Search results (if triggered)

### Tier 3: The Persona Core (Deep Identity)
7) User profile blocks (from My personality.md)
8) Identity prompt (core beliefs, traits, dreams, emotions)

### Tier 4: The Active Control (Final Mood)
9) Selected personality text (mood setting, only when Ctrl+P is enabled)
10) Chat history (user/assistant messages)

**Key principle**: The identity is stated FIRST, before the model sees any other information. This prevents confusion from model names or other identifiers that might appear in the system prompt or context. Simple and direct: "You are Kimi" before anything else.
