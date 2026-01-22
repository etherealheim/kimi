# My To-Do List

## High Priority

### Fix History Page
- [ ] Debug and fix issues with the history page functionality

### Evolving Personality Feature
- [ ] Implement personality that modifies itself based on user responses

#### Architecture: Context Block Expansion (Recommended Starting Point)

**Goal**: Auto-generate and update context blocks in "My personality.md" based on conversation patterns

**Phase 1: Preference Detection**
- [ ] Extract preference statements from user messages
  - Pattern matching: "I like/prefer/hate/always/never [thing]"
  - Store in database with confidence scores
- [ ] Detect recurring topics not in existing context blocks
- [ ] Use existing retrieval/embeddings system to identify themes

**Phase 2: Personality Updates**
- [ ] Create `personality_evolution` table in storage
  - Columns: topic, statement, confidence, timestamp, approved
- [ ] Implement `evolve_personality_from_message()` function
  - Extract learnings from each conversation
  - Store in database for review
- [ ] Add personality update consolidation (nightly job)
  - Deduplicate similar statements
  - Merge confirmed learnings into context blocks

**Phase 3: Review & Control**
- [ ] Build UI for reviewing proposed personality changes
  - Show what the system learned
  - Approve/reject/edit before applying
- [ ] Add personality version history
  - Track changes over time
  - Allow rollback to previous versions
- [ ] Implement confidence thresholds (only update after N confirmations)

**Phase 4: Advanced Features**
- [ ] Trait-based learning (adjust Warmth, Playfulness, etc. based on interaction patterns)
- [ ] Time-based context (morning person, night owl patterns)
- [ ] Relationship evolution (tone becomes more familiar over time)

**Technical Considerations**
- Drift prevention: Don't let personality become unrecognizable
- Conflict resolution: Handle contradictory preferences
- Manual override: Always allow direct file editing
- Update frequency: Start with batch processing, move to real-time later

**Example Use Case**:
```
User: "I really hate mushrooms"
↓
System extracts: {topic: "food", preference: "dislikes mushrooms", confidence: 0.8}
↓
After confirmation, updates personality:
[context:food]
Dislikes: mushrooms
Loves: spicy food
```

---

## Notes
- Start with Phase 1 (detection) before building full update pipeline
- Leverage existing `src/services/retrieval.rs` and embeddings system
- Keep personality file human-readable (markdown format)
- Consider using LLM to analyze patterns every 10 conversations
