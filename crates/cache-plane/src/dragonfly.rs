//! DragonflyDB Integration
//!
//! L2 distributed cache for encoded responses.
//! Uses Redis protocol (Dragonfly is Redis-compatible).

use hyperplane_types::{AccountEncoding, CommitmentLevel};
use redis::{aio::ConnectionManager, Client, RedisResult};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tracing::{debug, error};

/// DragonflyDB cache client
pub struct DragonflyCache {
    client: Option<Client>,
    connection: Arc<tokio::sync::Mutex<Option<ConnectionManager>>>,
    prefix: String,
    default_ttl_secs: u64,
}

impl DragonflyCache {
    /// Create new Dragonfly cache client
    pub async fn new(url: &str, prefix: &str, default_ttl_secs: u64) -> Result<Self, String> {
        let client = Client::open(url).map_err(|e| e.to_string())?;
        
        Ok(Self {
            client: Some(client),
            connection: Arc::new(tokio::sync::Mutex::new(None)),
            prefix: prefix.to_string(),
            default_ttl_secs,
        })
    }

    /// Get connection
    async fn get_connection(&self) -> Option<ConnectionManager> {
        let mut conn_guard = self.connection.lock().await;
        
        if let Some(conn) = conn_guard.as_ref() {
            // Check if connection is still valid
            // For simplicity, we'll reconnect on failure
            Some(conn.clone())
        } else if let Some(client) = &self.client {
            match client.get_connection_manager().await {
                Ok(conn) => {
                    *conn_guard = Some(conn.clone());
                    Some(conn)
                }
                Err(e) => {
                    error!("Failed to get Dragonfly connection: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Generate cache key
    fn make_key(&self, method: &str, key: &str) -> String {
        format!("{}:{}:{}", self.prefix, method, key)
    }

    /// Cache encoded response
    pub async fn set(
        &self,
        method: &str,
        key: &str,
        value: &[u8],
        ttl_secs: Option<u64>,
    ) -> RedisResult<()> {
        let cache_key = self.make_key(method, key);
        let ttl = ttl_secs.unwrap_or(self.default_ttl_secs);
        
        if let Some(mut conn) = self.get_connection().await {
            let result: RedisResult<()> = redis::cmd("SET")
                .arg(&cache_key)
                .arg(value)
                .arg("EX")
                .arg(ttl)
                .query_async(&mut conn)
                .await;
            
            match result {
                Ok(_) => {
                    debug!("Cached {} (ttl={}s)", cache_key, ttl);
                    Ok(())
                }
                Err(e) => {
                    error!("Failed to cache {}: {}", cache_key, e);
                    Err(e)
                }
            }
        } else {
            debug!("Dragonfly connection unavailable, skipping cache set");
            Ok(())
        }
    }

    /// Get cached response
    pub async fn get(&self, method: &str, key: &str) -> RedisResult<Option<Vec<u8>>> {
        let cache_key = self.make_key(method, key);
        
        if let Some(mut conn) = self.get_connection().await {
            let result: RedisResult<Option<Vec<u8>>> = redis::cmd("GET")
                .arg(&cache_key)
                .query_async(&mut conn)
                .await;
            
            match result {
                Ok(value) => {
                    if value.is_some() {
                        debug!("Cache hit for {}", cache_key);
                    } else {
                        debug!("Cache miss for {}", cache_key);
                    }
                    Ok(value)
                }
                Err(e) => {
                    debug!("Redis error: {}", e);
                    Err(e)
                }
            }
        } else {
            debug!("Dragonfly connection unavailable, cache miss");
            Ok(None)
        }
    }

    /// Delete cached entry
    pub async fn delete(&self, method: &str, key: &str) -> RedisResult<usize> {
        let cache_key = self.make_key(method, key);
        
        if let Some(mut conn) = self.get_connection().await {
            let deleted: usize = redis::cmd("DEL")
                .arg(&cache_key)
                .query_async(&mut conn)
                .await?;
            
            if deleted > 0 {
                debug!("Deleted {}", cache_key);
            }
            
            Ok(deleted)
        } else {
            Ok(0)
        }
    }

    /// Delete multiple keys by pattern
    pub async fn delete_pattern(&self, pattern: &str) -> RedisResult<usize> {
        let pattern = format!("{}:*:{}*", self.prefix, pattern);
        
        if let Some(mut conn) = self.get_connection().await {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg(&pattern)
                .query_async(&mut conn)
                .await?;
            
            if keys.is_empty() {
                return Ok(0);
            }
            
            let deleted: usize = redis::cmd("DEL")
                .arg(&keys)
                .query_async(&mut conn)
                .await?;
            
            debug!("Deleted {} keys matching {}", deleted, pattern);
            Ok(deleted)
        } else {
            Ok(0)
        }
    }

    /// Invalidate all caches for a pubkey
    pub async fn invalidate_pubkey(&self, pubkey: Pubkey) -> RedisResult<usize> {
        let pattern = pubkey.to_string();
        self.delete_pattern(&pattern).await
    }

    /// Get cache stats (requires Dragonfly ADMIN commands)
    pub async fn stats(&self) -> Option<DragonflyStats> {
        if let Some(mut conn) = self.get_connection().await {
            // Dragonfly-specific INFO command
            let info: String = redis::cmd("INFO")
                .arg("stats")
                .query_async(&mut conn)
                .await
                .ok()?;
            
            Some(DragonflyStats::parse(&info))
        } else {
            None
        }
    }

    /// Ping cache
    pub async fn ping(&self) -> bool {
        if let Some(mut conn) = self.get_connection().await {
            let result: RedisResult<String> = redis::cmd("PING")
                .query_async(&mut conn)
                .await;
            result.is_ok()
        } else {
            false
        }
    }

    /// Close connection
    pub async fn close(&self) {
        let mut conn_guard = self.connection.lock().await;
        *conn_guard = None;
    }
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct DragonflyStats {
    pub hits: u64,
    pub misses: u64,
    pub keys: u64,
    pub memory_bytes: u64,
}

impl DragonflyStats {
    fn parse(info: &str) -> Self {
        let mut stats = Self::default();
        
        for line in info.lines() {
            if let Some((key, value)) = line.split_once(':') {
                match key.trim() {
                    "keyspace_hits" => stats.hits = value.parse().unwrap_or(0),
                    "keyspace_misses" => stats.misses = value.parse().unwrap_or(0),
                    "keys" => stats.keys = value.parse().unwrap_or(0),
                    "used_memory" => stats.memory_bytes = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
        
        stats
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Generate cache key for getAccountInfo
pub fn make_account_info_key(
    pubkey: Pubkey,
    encoding: AccountEncoding,
    commitment: CommitmentLevel,
) -> String {
    format!(
        "ai:{}:{}:{}",
        pubkey,
        encoding as u8,
        commitment as u8
    )
}

/// Generate cache key for getProgramAccounts
pub fn make_program_accounts_key(
    program_id: Pubkey,
    filters_hash: &[u8],
    encoding: AccountEncoding,
    commitment: CommitmentLevel,
    cursor: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};
    
    let mut hasher = Sha256::new();
    hasher.update(&program_id.to_bytes());
    hasher.update(filters_hash);
    hasher.update(&[encoding as u8]);
    hasher.update(&[commitment as u8]);
    if let Some(c) = cursor {
        hasher.update(c.as_bytes());
    }
    let hash = hasher.finalize();
    
    format!(
        "pa:{}:{}:{}:{}",
        program_id,
        hex::encode(&hash[..8]),
        encoding as u8,
        commitment as u8
    )
}

// Helper for hex encoding
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let pubkey = Pubkey::new_unique();
        let key = make_account_info_key(pubkey, AccountEncoding::Base64, CommitmentLevel::Processed);
        
        assert!(key.starts_with("ai:"));
        assert!(key.contains(&pubkey.to_string()));
    }

    #[test]
    fn test_program_accounts_key() {
        let program_id = Pubkey::new_unique();
        let filters_hash = vec![1u8; 32];
        let key = make_program_accounts_key(
            program_id,
            &filters_hash,
            AccountEncoding::Base64,
            CommitmentLevel::Processed,
            None,
        );
        
        assert!(key.starts_with("pa:"));
        assert!(key.contains(&program_id.to_string()));
    }
}
