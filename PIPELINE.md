# Kimi Message Processing Pipeline

Complete flow from user input to LLM response with all decision points and context sources.

## Pipeline Overview

```
User Message
    ↓
[1] Input Processing & Command Detection
    ↓
[2] Deterministic Handlers (Time/Date/Weather)
    ↓
[3] Context Building (Memories, Obsidian, History, Search)
    ↓
[4] Prompt Assembly
    ↓
[5] LLM Call (Primary)
    ↓
[6] Verification Pass (Optional Two-Pass)
    ↓
[7] Response Display
```

---

## Stage 1: Input Processing & Command Detection
**File**: `src/app/chat/input.rs::send_chat_message()`

### Flow:
1. **Empty check** - Return if input is empty
2. **Command shortcuts**:
   - `/convert` → File conversion command
   - `/download` → Link download command
   - If handled, add to history immediately & return
3. **Clean input**:
   - Extract message content
   - Parse image attachments (`[[image:...]]` tokens)
   - Clear input field
4. **Add to chat history** - User message visible immediately

---

## Stage 2: Deterministic Handlers
**File**: `src/app/chat/input.rs`

### Fast path for factual queries (no LLM needed):

#### Time Questions (`try_handle_time_question`)
- Triggers: "what time", "current time", "time is it"
- **Returns**: `"It's 20:15:42 CET."` (instant)

#### Date Questions (`try_handle_date_question`)
- Triggers: "what date", "what day is", "today", "tomorrow", "yesterday"
- **Uses**: `chrono` for calculations
- **Returns**: `"Today is Wednesday, January 21, 2026."` (instant)

#### Weather Questions (`try_handle_weather_question`)
- Triggers: "weather", "forecast", "temperature", "temp", "rain", "wind"
- Location filter: Prague only (hardcoded)
- **Calls**: `WeatherService::fetch_current_weather_json()`
- **Returns**: `"Current weather in Prague: -6.0°C, wind 15 km/h (as of 20:00)."` (instant)

**If any handler triggers → Skip LLM entirely, return immediately**

---

## Stage 3: Context Building
**File**: `src/app/chat/agent.rs::build_agent_messages()`

### 3.1 Base System Prompt Setup

```rust
let mut prompt_lines = vec![
    agent.system_prompt,  // e.g., "You are a helpful assistant"
    "Current date and time: 2026-01-21 20:15:42",
    "Respond in plain text. Do not use Markdown formatting.",
    "Respond in English unless the user asks otherwise.",
];
```

### 3.2 User Profile Context (My Personality)
**File**: `src/services/personality.rs` → `data/my_personality.txt`

Parses blocks:
- `[always]` - Always included in prompt
- `[context:tag]` - Included if query contains tag or keywords from content

**Example**:
```
[always]
I live in Prague, Czech Republic.

[context:work]
I work at IndieWeb.org as a developer.
```

**Decision**: Keyword match on query → Include tagged context

---

### 3.3 Conversation History Summaries
**File**: `src/app/chat/agent/context.rs::build_conversation_summary_entries()`

**Triggers**: Query contains recap/summary intent
- "what did we discuss", "recap", "summary", "what happened"
- Time ranges: "today", "yesterday", "this week", "last week", "X days ago"

**Process**:
1. Detect time range from query keywords
2. Load all conversation summaries from SQLite (`data/history.db`)
3. Filter by date range using ISO week boundaries (from `dates.rs`)
4. Format as bullet list: `- 2026-01-21: Discussed routing pipeline`

**Count**: Number of summary entries

**Prompt injection**:
```
--- Conversation summaries ---
- 2026-01-20: Created date parser module
- 2026-01-19: Fixed weekly note retrieval

Use the summaries above to answer recap questions.
```

---

### 3.4 Memories Context
**File**: `src/app/chat/agent/context.rs::build_memory_context()`
**Source**: `data/memories.txt`

**Structure**:
```
[context:likes]
I prefer dark mode
I like minimal UIs

[context:projects]
Working on Kimi TUI app
Learning Rust
```

**Process**:
1. Tokenize query (split, lowercase, filter short words)
2. For each `[context:tag]`:
   - **Tag match**: If query contains tag name → Include entire section
   - **Line match**: Otherwise, include only lines matching query tokens
3. Count matching **sections** (not individual lines)

**Count**: Number of context sections used (e.g., `likes` + `projects` = 2)

**Prompt injection**:
```
--- Memories ---
[context:likes]
I prefer dark mode
I like minimal UIs

Use the memories above as persistent user facts.
```

---

### 3.5 Obsidian Notes Context
**File**: `src/app/chat/agent/obsidian.rs::build_obsidian_context()`

#### Decision Tree:

**External events query?** (`is_external_event_query`)
- Triggers: "happening", "news" + "today"/"current" + location
- Example: "what's happening today in Prague?"
- **Action**: Skip Obsidian → Go to web search

**Week note query?** (`is_week_note_query` or `is_personal_recap_query`)
- Triggers: "weekly note", "this week", "my week", "2026-W4"
- **Action**: Fetch week-specific notes

**General query?**
- **Action**: Fuzzy search across all notes

#### Week Note Flow:
**File**: `src/services/dates.rs::resolve_query_week()`

1. **Parse explicit week**: `"2026-W4"` → 2026, week 4
2. **"last week"**: Calculate previous ISO week (handles year boundaries)
3. **Default**: Current ISO week

**File**: `src/services/obsidian.rs::week_notes_context()`

1. Calculate week date range (Monday-Sunday)
2. Scan vault for markdown files
3. Filter by note type:
   - **Daily notes**: `YYYY-MM-DD` format, must fall in week range
   - **Weekly notes**: `YYYY-WNN` format, must match ISO week
4. Extract content:
   - Weekly notes: 80 lines (MAX_DETAIL_LINES)
   - Daily notes: 12 lines (MAX_WEEK_NOTE_LINES)

**Checklist query?** (`"checklist"`, `"todo"`, `"tasks"`)
- Extract `- [ ]` and `- [x]` items from weekly note

**Count**: Number of notes retrieved

**Prompt injection**:
```
--- Obsidian weekly notes ---
2026-W04 (weekly note)
## Goals
- Finish routing pipeline
- Test date parser

2026-01-20 (daily note)
Worked on date parsing logic.

Use the Obsidian notes below to answer questions about the user's notes.
Do not add or infer information that is not explicitly present.
```

#### General Note Search Flow:
**File**: `src/services/obsidian.rs::search_notes()`

1. Tokenize query (same as memory context)
2. Scan all markdown files in vault (recursive)
3. Score each file:
   - Content matches: +2 per token occurrence
   - Title matches: +3 per token
4. Sort by score, take top 5
5. Extract snippets:
   - Find line with token match
   - Extract context (1 line before + max_lines after)
   - Detail queries get 80 lines, others get 6

**Count**: Number of notes in results

---

### 3.6 Web Search Context
**File**: `src/app/chat/agent/search.rs::enrich_prompt_with_search()`

#### Skip Conditions:
- Personal recap query (`"my week"`, `"recap"`)
- Week note query (`"weekly note"`, `"2026-W4"`)

#### External Event (Force Search):
- Triggers: "happening", "news" + "today"/"current" + location
- **Action**: Immediate Brave search

#### LLM Decision Router:
**File**: `decide_search_decision()`

**System Prompt**:
```
You are a search routing assistant.
Decide whether a user's request needs live web search.

Return JSON: {"action":"search|direct|clarify","query":"..."}

Rules:
- "search" for proper nouns, companies, products, recent info
- "clarify" if ambiguous
- "direct" if general knowledge
```

**LLM Call**: Quick classification (same agent as chat)

**Response parsing**:
- `{"action":"search","query":"prague weather"}` → Search
- `{"action":"direct"}` → No search
- `{"action":"clarify"}` → Add "Ask a brief clarifying question" to prompt

#### Fallback Heuristics (if LLM fails):
**File**: `should_use_brave_search()`

**Search if**:
- Entity query: CamelCase, digits, separators, ≤4 words
- Search terms: "latest", "current", "news", "happening", "price", "event"
- Time cues: "2024", "2025", "this week"
- Question + location: "what ... in Prague?"

**Skip if**:
- Weather question (handled deterministically)

#### Brave Search Execution:
**File**: `append_brave_search_results()`

1. Check API key configured
2. Call `crate::services::brave::search(api_key, query)`
3. Parse results (titles, URLs, snippets)

**Prompt injection**:
```
All temperatures must be in Celsius (metric units).
Use only the search results below to answer.
If they are missing, say you cannot find up-to-date information.

Brave search results for "prague weather 2026":
1. Weather.com - Prague: Current temperature -6°C...
2. AccuWeather - Prague forecast: Cold snap continues...
```

**Error handling**:
- No API key → `pending_search_notice` = "Add Brave API key"
- Empty results → `"I couldn't find any live search results"`
- API error → `"Live search failed: [error]"`

**If `pending_search_notice` set → Abort LLM call, show notice**

---

### 3.7 AI Personality (Optional)
**File**: `src/services/personality.rs` → `data/personalities/[name].txt`

**Enabled if**: User toggled personality mode ON

**Content**: Character persona text appended to prompt
```
You are Kimi, a friendly and knowledgeable assistant.
You speak concisely and avoid unnecessary formality.
```

---

## Stage 4: Prompt Assembly
**File**: `src/app/chat/agent.rs::build_agent_messages()`

### Final Prompt Structure:

```
System message:
┌─────────────────────────────────────────────────┐
│ You are a helpful assistant.                    │
│                                                  │
│ Current date and time: 2026-01-21 20:15:42      │
│ Respond in plain text. Do not use Markdown.     │
│ Respond in English unless asked otherwise.      │
│                                                  │
│ User context (always):                           │
│ I live in Prague, Czech Republic.               │
│                                                  │
│ --- Conversation summaries ---                  │
│ - 2026-01-20: Discussed routing pipeline        │
│ Use the summaries above to answer recap Qs.     │
│                                                  │
│ --- Memories ---                                │
│ [context:likes]                                 │
│ I prefer dark mode                              │
│ Use the memories above as persistent facts.     │
│                                                  │
│ --- Obsidian weekly notes ---                   │
│ 2026-W04 (weekly note)                          │
│ ## Goals                                        │
│ - Test date parser                              │
│ Use notes to answer questions about my notes.   │
│ Do not add or infer information not present.    │
│                                                  │
│ All temperatures must be in Celsius.            │
│ Use only the search results below to answer.    │
│ Brave search results for "prague weather":      │
│ 1. Weather.com - Current: -6°C...               │
│                                                  │
│ You are Kimi, a friendly assistant.             │
└─────────────────────────────────────────────────┘

User: what notes did i write last week?
Assistant: Based on your notes from last week...
User: <current query>
```

### Message Array:
```rust
messages = [
    AgentChatMessage::system(merged_prompt),
    AgentChatMessage::user("previous user message"),
    AgentChatMessage::assistant("previous assistant response"),
    // ... full chat history ...
    AgentChatMessage::user("current query"),
]
```

### Image Attachments:
If `chat_attachments` present:
- Read image bytes
- Base64 encode
- Attach to last user message: `messages[last].images = [base64_image]`

### Context Usage Tracking:
```rust
ContextUsage {
    notes_used: 3,      // Number of Obsidian notes
    history_used: 2,    // Number of conversation summaries
    memories_used: 1,   // Number of memory sections
}
```
Stored in `pending_context_usage` for UI display

### Should Verify Flag:
```rust
should_verify = has_context_usage || has_search_context
```
Determines if two-pass verification is needed

---

## Stage 5: LLM Call (Primary)
**File**: `src/app/chat/agent.rs::spawn_agent_chat_thread()`

### Background Thread:
```rust
std::thread::spawn(move || {
    let response = manager.chat(&agent, &messages)?;
    // ... verification logic ...
    agent_tx.send(AgentEvent::Response(response))
});
```

### Agent Manager:
**File**: `src/agents/ollama.rs` or `src/agents/venice.rs`

**Ollama**:
- Endpoint: `http://localhost:11434/api/chat`
- Streaming: Disabled (collects full response)
- Model: User-selected (e.g., `qwen2.5:14b`)

**Venice**:
- Endpoint: `https://api.venice.ai/api/v1/chat/completions`
- Headers: `Authorization: Bearer [api_key]`
- Model: User-selected (e.g., `llama-3.3-70b`)

### Response:
Raw text from LLM (no markdown processing)

---

## Stage 6: Verification Pass (Two-Pass RAG)
**File**: `src/app/chat/agent/verification.rs`

### Trigger Condition:
```rust
if should_verify 
   && !response.trim().is_empty() 
   && should_verify_response(&system_context)
```

**`should_verify_response` checks**:
```rust
system_context.contains("--- Memories ---")
|| system_context.contains("--- Conversation summaries ---")
|| system_context.contains("--- Obsidian")
|| system_context.contains("Brave search results for")
```

### Verification Prompt:
```
System: You verify responses against provided context. Respond in English only.

User:
Context:
[Full system_context from Stage 4]

Original response:
[LLM's first response]

Verify the response using only the provided context.
Correct any statements not supported by the context.
If nothing needs correction, return the original response.
Respond in English. Output only the final response text.
```

### Second LLM Call:
Same agent, same manager → Verification response

### Result:
- If verification succeeds → Use verified response
- If verification fails → Use original response
- If verified response empty → Use original response

**Key**: Only current `system_context` + `response` are in verification prompt
- No prior chat history (prevents hallucination from old conversations)

---

## Stage 7: Response Display
**File**: `src/app/chat/response.rs::handle_agent_response()`

### Event Handling:
```rust
match event {
    AgentEvent::Response(content) => {
        self.is_loading = false;
        self.is_searching = false;
        self.is_analyzing = false;
        
        let context_usage = self.pending_context_usage.take();
        
        self.chat_history.push(ChatMessage {
            role: MessageRole::Assistant,
            content,
            timestamp: Local::now().format("%H:%M:%S"),
            display_name: personality_name,
            context_usage,  // Shows as "3n | 2h | 1m"
        });
        
        self.save_to_storage();
    }
    AgentEvent::Error(error) => {
        self.add_system_message(&format!("Error: {}", error));
    }
}
```

### UI Display:
**File**: `src/ui/chat.rs::render_regular_message()`

```
< Kimi  20:15:42  3n | 2h | 1m
Based on your weekly notes from 2026-W03, you worked on...
```

**Context usage indicators**:
- `3n` = 3 Obsidian notes used
- `2h` = 2 conversation summaries used
- `1m` = 1 memory section used
- Gray color, right-aligned in header

---

## Decision Matrix Summary

| Query Type | Deterministic | Obsidian | History | Search | Verify |
|------------|--------------|----------|---------|--------|--------|
| "what time is it?" | ✅ Time handler | ❌ | ❌ | ❌ | ❌ |
| "weather in prague?" | ✅ Weather API | ❌ | ❌ | ❌ | ❌ |
| "what did i write last week?" | ❌ | ✅ Week notes | ❌ | ❌ | ✅ |
| "summarize my week" | ❌ | ✅ Week notes | ✅ Summaries | ❌ | ✅ |
| "what happened today in Prague?" | ❌ | ❌ | ❌ | ✅ Brave | ✅ |
| "explain Rust lifetimes" | ❌ | 🟡 If notes exist | ❌ | 🟡 LLM decides | 🟡 If context |
| "what do i like?" | ❌ | ❌ | ❌ | ❌ | ✅ |

Legend:
- ✅ Always triggers
- ❌ Never triggers
- 🟡 Conditionally triggers

---

## Key Design Principles

### 1. Speed Hierarchy
- **Instant** (0ms): Deterministic handlers (time/date/weather)
- **Fast** (<100ms): Database queries (history summaries)
- **Medium** (~1s): File I/O (Obsidian, memories)
- **Slow** (~3s): LLM calls (search decision, verification)
- **Slowest** (~5-10s): Primary LLM generation

### 2. Accuracy Guarantees
- **Factual queries**: Deterministic handlers (100% accurate)
- **Personal data**: Obsidian + memories (user-controlled truth)
- **Live events**: Web search (up-to-date external data)
- **General knowledge**: LLM (subject to hallucination)
- **Verification**: Two-pass for context-heavy queries

### 3. Context Prioritization
1. Always-on user profile
2. Memories (persistent facts)
3. Conversation summaries (recent context)
4. Obsidian notes (user's knowledge base)
5. Web search (live external data)
6. AI personality (tone/style)

### 4. Error Handling
- Missing API keys → User notice, skip service
- Empty results → Graceful fallback message
- LLM failures → Display error, don't crash
- File I/O errors → Log, continue with other sources

### 5. Privacy & Control
- All user data stored locally (`data/` folder)
- API keys in `config.local.toml` (gitignored)
- Obsidian vault: read-only access
- No data sent to LLM without user query trigger

---

## Performance Optimizations

1. **Parallel file reads**: Memories, personality, Obsidian can be read concurrently
2. **Lazy loading**: Personality text loaded only when enabled
3. **Token caching**: Query tokenization done once, reused for all context sources
4. **Early exits**: Deterministic handlers skip entire LLM pipeline
5. **Streaming skipped**: Full response collected for verification pass (trade-off)
6. **Background threads**: LLM calls don't block UI
7. **Date calculations**: Centralized in `dates.rs` module (no recalculation)

---

## Potential Improvements

### Current Issues:
1. **No caching**: Every query re-reads all files (memories, personality, etc.)
2. **Sequential LLM calls**: Search decision → Primary → Verification (could batch)
3. **No smart truncation**: Large Obsidian notes capped at char limit (may cut mid-sentence)
4. **Memories count semantics**: Counts sections, not relevance strength
5. **Search decision overhead**: Extra LLM call even for simple queries

### Suggested Enhancements:
1. **Context caching**: Cache file reads with file watcher invalidation
2. **Parallel LLM calls**: Run search decision + primary in parallel
3. **Smart summarization**: If context exceeds limit, use LLM to summarize before injecting
4. **Relevance scoring**: Weight context sections by match score, not binary inclusion
5. **Heuristic-first routing**: Only use LLM decision for ambiguous queries
6. **Token counting**: Track actual token usage vs. model limits
7. **Context pruning**: Intelligently drop low-relevance context if prompt too large
