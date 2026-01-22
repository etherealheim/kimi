//! Standalone debug tool to check Kimi's database and retrieval pipeline
//! Run with: cargo run (from the tools/debug-db folder)
//!
//! Make sure Kimi is NOT running (database lock)

use serde::{Deserialize, Serialize};
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::Surreal;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct CountResult {
    count: usize,
}

#[derive(Debug, Deserialize)]
struct MessageRow {
    content: String,
    role: String,
    timestamp: String,
    has_embedding: bool,
}

#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
struct SimilarResult {
    content: String,
    role: String,
    timestamp: String,
    similarity: f32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Kimi Database Debug Tool ===\n");

    // 1. Check Ollama
    println!("1. Checking Ollama...");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    
    let ollama_ok = match client.get("http://localhost:11434/api/tags").send().await {
        Ok(resp) if resp.status().is_success() => {
            println!("   Ollama: RUNNING");
            if let Ok(text) = resp.text().await {
                if text.contains("mxbai-embed-large") {
                    println!("   mxbai-embed-large: FOUND");
                    true
                } else {
                    println!("   mxbai-embed-large: NOT FOUND - run 'ollama pull mxbai-embed-large'");
                    false
                }
            } else {
                false
            }
        }
        _ => {
            println!("   Ollama: NOT RUNNING");
            false
        }
    };

    // 2. Test embedding generation
    println!("\n2. Testing embedding generation...");
    let test_embedding = if ollama_ok {
        match client
            .post("http://localhost:11434/api/embed")
            .json(&EmbedRequest {
                model: "mxbai-embed-large".to_string(),
                input: "I like apples".to_string(),
            })
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<EmbedResponse>().await {
                    Ok(data) if !data.embeddings.is_empty() => {
                        let emb = &data.embeddings[0];
                        println!("   SUCCESS: {} dimensions", emb.len());
                        println!("   Sample values: [{:.4}, {:.4}, {:.4}, ...]", emb[0], emb[1], emb[2]);
                        Some(emb.clone())
                    }
                    _ => {
                        println!("   FAILED: No embedding in response");
                        None
                    }
                }
            }
            Ok(resp) => {
                println!("   FAILED: HTTP {}", resp.status());
                None
            }
            Err(e) => {
                println!("   FAILED: {}", e);
                None
            }
        }
    } else {
        println!("   SKIPPED (Ollama not available)");
        None
    };

    // 3. Open database
    println!("\n3. Opening database...");
    let db_path = PathBuf::from("/home/ethereal/git-local/kimi/data/kimi.db");
    if !db_path.exists() {
        println!("   ERROR: Database not found at {:?}", db_path);
        return Ok(());
    }
    
    let db: Surreal<Db> = Surreal::new::<RocksDb>(db_path).await?;
    db.use_ns("kimi").use_db("main").await?;
    println!("   SUCCESS: Connected to database");

    // 4. Count messages
    println!("\n4. Message statistics...");
    
    let mut resp = db.query("SELECT count() AS count FROM message GROUP ALL").await?;
    let total: Vec<CountResult> = resp.take(0)?;
    let total_count = total.first().map(|c| c.count).unwrap_or(0);
    println!("   Total messages: {}", total_count);
    
    let mut resp = db.query("SELECT count() AS count FROM message WHERE embedding IS NOT NONE GROUP ALL").await?;
    let with_emb: Vec<CountResult> = resp.take(0)?;
    let with_emb_count = with_emb.first().map(|c| c.count).unwrap_or(0);
    println!("   With embeddings: {}", with_emb_count);
    println!("   Without embeddings: {}", total_count - with_emb_count);

    // 5. Show recent messages
    println!("\n5. Recent messages...");
    let mut resp = db.query("
        SELECT 
            content, 
            role, 
            timestamp,
            embedding IS NOT NONE AS has_embedding
        FROM message 
        ORDER BY timestamp DESC 
        LIMIT 10
    ").await?;
    let messages: Vec<MessageRow> = resp.take(0)?;
    
    if messages.is_empty() {
        println!("   NO MESSAGES IN DATABASE!");
        println!("   This is the problem - no conversations have been saved.");
    } else {
        for (i, msg) in messages.iter().enumerate() {
            let preview: String = msg.content.chars().take(50).collect();
            let emb_status = if msg.has_embedding { "✓" } else { "✗" };
            println!(
                "   {}. [{}] {} emb={} | {}...",
                i + 1, msg.timestamp, msg.role, emb_status, preview
            );
        }
    }

    // 6. Test vector search with "what do i like" (the actual failing query)
    println!("\n6. Testing vector search for 'what do i like'...");
    if with_emb_count > 0 {
        // Generate embedding for the actual query that fails
        let query_emb = match client
            .post("http://localhost:11434/api/embed")
            .json(&EmbedRequest {
                model: "mxbai-embed-large".to_string(),
                input: "what do i like".to_string(),
            })
            .send()
            .await
        {
            Ok(resp) => resp.json::<EmbedResponse>().await.ok().and_then(|r| r.embeddings.into_iter().next()),
            Err(_) => None,
        };
        
        let query_emb = match query_emb {
            Some(e) => e,
            None => {
                println!("   Failed to generate embedding for query");
                return Ok(());
            }
        };
        
        let mut resp = db.query("
            SELECT 
                content,
                role,
                timestamp,
                vector::similarity::cosine(embedding, $query_embedding) AS similarity
            FROM message
            WHERE embedding IS NOT NONE
            ORDER BY similarity DESC
            LIMIT 5
        ")
        .bind(("query_embedding", query_emb))
        .await?;
        
        let similar: Vec<SimilarResult> = resp.take(0)?;
        if similar.is_empty() {
            println!("   No results from vector search");
        } else {
            println!("   Top {} similar messages:", similar.len());
            for (i, s) in similar.iter().enumerate() {
                let preview: String = s.content.chars().take(50).collect();
                println!(
                    "   {}. sim={:.4} [{}] {} | {}...",
                    i + 1, s.similarity, s.timestamp, s.role, preview
                );
            }
        }
    } else if with_emb_count == 0 {
        println!("\n6. Vector search: SKIPPED (no messages have embeddings)");
        println!("   Messages are saved but embeddings are not being generated!");
    }

    // 7. Test keyword search
    println!("\n7. Testing keyword search for 'like'...");
    let mut resp = db.query("
        SELECT content, role, timestamp
        FROM message
        WHERE content @@ 'like'
        LIMIT 5
    ").await?;
    
    #[derive(Debug, Deserialize)]
    struct KeywordResult {
        content: String,
        role: String,
        timestamp: String,
    }
    let keyword_results: Vec<KeywordResult> = resp.take(0)?;
    if keyword_results.is_empty() {
        println!("   No results for keyword 'like'");
    } else {
        println!("   Found {} messages containing 'like':", keyword_results.len());
        for (i, r) in keyword_results.iter().enumerate() {
            let preview: String = r.content.chars().take(50).collect();
            println!("   {}. [{}] {} | {}...", i + 1, r.timestamp, r.role, preview);
        }
    }

    // 8. Check conversations
    println!("\n8. Conversations...");
    let mut resp = db.query("SELECT count() AS count FROM conversation GROUP ALL").await?;
    let conv_count: Vec<CountResult> = resp.take(0)?;
    println!("   Total conversations: {}", conv_count.first().map(|c| c.count).unwrap_or(0));

    // 9. Test heuristic fallback - find messages with "i like", "i love", etc.
    println!("\n9. Testing heuristic fallback (preference statements)...");
    let mut resp = db.query("
        SELECT content, role, timestamp
        FROM message
        WHERE role = 'User'
        ORDER BY timestamp DESC
        LIMIT 50
    ").await?;
    
    #[derive(Debug, Deserialize)]
    struct UserMsg {
        content: String,
        role: String,
        timestamp: String,
    }
    let user_msgs: Vec<UserMsg> = resp.take(0)?;
    
    let preference_patterns = ["i like ", "i love ", "i prefer ", "my favorite ", "my favourite "];
    let mut preference_msgs = Vec::new();
    for msg in &user_msgs {
        let lowered = msg.content.to_lowercase();
        if preference_patterns.iter().any(|p| lowered.contains(p)) {
            preference_msgs.push(msg);
        }
    }
    
    if preference_msgs.is_empty() {
        println!("   No preference statements found!");
    } else {
        println!("   Found {} preference statements:", preference_msgs.len());
        for (i, msg) in preference_msgs.iter().take(5).enumerate() {
            println!("   {}. [{}] {}", i + 1, msg.timestamp, msg.content);
        }
    }

    // 10. Direct similarity test between "what do i like" and "i like apples"
    println!("\n10. Direct similarity test...");
    if ollama_ok {
        async fn get_embedding(client: &reqwest::Client, text: &str) -> Option<Vec<f32>> {
            #[derive(serde::Serialize)]
            struct Req { model: String, input: String }
            #[derive(serde::Deserialize)]
            struct Resp { embeddings: Vec<Vec<f32>> }
            
            let resp = client
                .post("http://localhost:11434/api/embed")
                .json(&Req { model: "mxbai-embed-large".to_string(), input: text.to_string() })
                .send().await.ok()?;
            let data: Resp = resp.json().await.ok()?;
            data.embeddings.into_iter().next()
        }
        
        let emb1 = get_embedding(&client, "what do i like").await;
        let emb2 = get_embedding(&client, "i like apples").await;
            
        if let (Some(e1), Some(e2)) = (emb1, emb2) {
            let dot: f32 = e1.iter().zip(e2.iter()).map(|(a, b)| a * b).sum();
            let norm1: f32 = e1.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm2: f32 = e2.iter().map(|x| x * x).sum::<f32>().sqrt();
            let cosine_sim = dot / (norm1 * norm2);
            println!("   'what do i like' vs 'i like apples': {:.4}", cosine_sim);
            println!("   Threshold is 0.3, so this {} pass", if cosine_sim > 0.3 { "SHOULD" } else { "WON'T" });
        }
    }

    println!("\n=== Debug Complete ===");
    
    // Summary
    println!("\n=== DIAGNOSIS ===");
    if total_count == 0 {
        println!("PROBLEM: No messages in database.");
        println!("  -> Conversations are not being saved properly.");
        println!("  -> Check if you exit chats via Esc (to History) to trigger save.");
    } else if with_emb_count == 0 {
        println!("PROBLEM: Messages exist but none have embeddings.");
        println!("  -> Embedding generation during save might be failing.");
        println!("  -> The backfill mechanism should fix this on next query.");
    } else if with_emb_count < total_count / 2 {
        println!("WARNING: Only {}% of messages have embeddings.", with_emb_count * 100 / total_count);
        println!("  -> Backfill should gradually fix this.");
    } else {
        println!("Database looks healthy: {} messages, {} with embeddings", total_count, with_emb_count);
        if !ollama_ok {
            println!("BUT: Ollama is not running, so queries can't generate embeddings.");
        }
    }

    Ok(())
}
