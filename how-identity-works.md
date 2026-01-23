## Identity System Overview

The identity system is layered to mirror how a human psyche forms and evolves. Each layer has its own cadence and rules so the identity feels organic without becoming overwhelming.

## Layers

1) Core beliefs + backstory (manual only)
- Stable identity, worldview, and boundaries.
- Edited directly by the user.

2) Traits (semi-manual, slow drift)
- Traits are behavioral axes with a 0.0–1.0 strength.
- Explicit user requests can adjust a trait immediately.
- Inferred traits require repeated evidence.

3) Dreams + ambitions (gentle, capped)
- Active dreams are pursued softly.
- Backlog dreams are “brewing” ideas waiting for evidence.

4) Chat context (ephemeral)
- The current conversation context, used only for immediate replies.

## Where Data Lives

Identity state is stored in `data/identity-state.json`. This file is created automatically on first run.

## Identity vs Personality

- **Identity** is Kimi's core self (always active): core beliefs, traits, dreams
- **Personality** is an optional overlay (toggle with Ctrl+P): can be any persona (cat, monkey, etc.)

## Update Rules

### Core beliefs
- Manual only. The model never edits this layer.

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

## How Updates Happen

After a conversation summary is generated, a lightweight reflection step:
- Reads the current identity state.
- Reviews the conversation summary and recent user messages.
- Proposes minimal JSON updates (traits/dreams only).
- Applies caps and decay rules.

## Prompt Injection Order

Default prompt order:
1) Base system prompt
2) User profile blocks
3) Retrieved context (memories/notes/search)
4) Identity layers (core, traits, dreams)
5) Selected personality text (only when Ctrl+P is enabled)

This order keeps identity stable while allowing gentle evolution.
