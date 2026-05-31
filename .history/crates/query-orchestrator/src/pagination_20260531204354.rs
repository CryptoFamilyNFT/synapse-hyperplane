//! Pagination Support
//!
//! Handles pagination for getProgramAccounts queries:
//! - Cursor-based pagination
//! - Limit enforcement
//! - Page token generation

use serde::{Serialize, Deserialize};
use solana_sdk::pubkey::Pubkey;

/// Pagination cursor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationCursor {
    /// Last pubkey in previous page (for cursor-based pagination)
    pub last_pubkey: Option<Pubkey>,
    /// Offset from start (for offset-based pagination)
    pub offset: Option<u64>,
    /// Page size
    pub limit: usize,
}

impl PaginationCursor {
    pub fn new(limit: usize) -> Self {
        Self {
            last_pubkey: None,
            offset: None,
            limit,
        }
    }
    
    pub fn from_pubkey(pubkey: Pubkey, limit: usize) -> Self {
        Self {
            last_pubkey: Some(pubkey),
            offset: None,
            limit,
        }
    }
    
    pub fn from_offset(offset: u64, limit: usize) -> Self {
        Self {
            last_pubkey: None,
            offset: Some(offset),
            limit,
        }
    }
    
    /// Serialize cursor to base64 string
    pub fn to_base64(&self) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let json = serde_json::to_string(self).unwrap_or_default();
        STANDARD.encode(json.as_bytes())
    }
    
    /// Deserialize cursor from base64 string
    pub fn from_base64(s: &str) -> Option<Self> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let bytes = STANDARD.decode(s).ok()?;
        let json = String::from_utf8(bytes).ok()?;
        serde_json::from_str(&json).ok()
    }
}

/// Paginated query result
#[derive(Debug, Clone)]
pub struct PaginatedResult {
    /// Results for current page
    pub results: Vec<Pubkey>,
    /// Cursor for next page (if more results available)
    pub next_cursor: Option<PaginationCursor>,
    /// Total number of results (if known)
    pub total_count: Option<u64>,
    /// Whether this is the last page
    pub is_last_page: bool,
}

impl PaginatedResult {
    pub fn new(results: Vec<Pubkey>, limit: usize, total_count: Option<u64>) -> Self {
        let is_last_page = results.len() < limit;
        
        let next_cursor = if is_last_page {
            None
        } else {
            results.last().map(|last| PaginationCursor::from_pubkey(*last, limit))
        };
        
        Self {
            results,
            next_cursor,
            total_count,
            is_last_page,
        }
    }
}

/// Pagination helper
pub struct PaginationHelper {
    /// Default page size
    default_limit: usize,
    /// Maximum page size
    max_limit: usize,
}

impl PaginationHelper {
    pub fn new(default_limit: usize, max_limit: usize) -> Self {
        Self {
            default_limit,
            max_limit,
        }
    }
    
    /// Validate and adjust limit
    pub fn validate_limit(&self, limit: Option<usize>) -> usize {
        match limit {
            None => self.default_limit,
            Some(l) => l.min(self.max_limit),
        }
    }
    
    /// Apply pagination to results
    pub fn paginate(
        &self,
        results: &[Pubkey],
        cursor: Option<&PaginationCursor>,
    ) -> PaginatedResult {
        let limit = cursor.map(|c| c.limit).unwrap_or(self.default_limit);
        
        let start_index = match cursor {
            Some(c) if c.last_pubkey.is_some() => {
                // Cursor-based: find position after last_pubkey
                if let Some(last_pubkey) = c.last_pubkey {
                    results.iter()
                        .position(|p| p == &last_pubkey)
                        .map(|pos| pos + 1)
                        .unwrap_or(0)
                } else {
                    0
                }
            }
            Some(c) if c.offset.is_some() => {
                // Offset-based
                c.offset.unwrap_or(0) as usize
            }
            _ => 0,
        };
        
        let end_index = (start_index + limit).min(results.len());
        
        let paginated_results = if start_index >= results.len() {
            Vec::new()
        } else {
            results[start_index..end_index].to_vec()
        };
        
        PaginatedResult::new(
            paginated_results,
            limit,
            Some(results.len() as u64),
        )
    }
}

impl Default for PaginationHelper {
    fn default() -> Self {
        Self::new(100, 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pagination_cursor_serialization() {
        let cursor = PaginationCursor::from_pubkey(Pubkey::new_unique(), 100);
        let encoded = cursor.to_base64();
        let decoded = PaginationCursor::from_base64(&encoded).unwrap();
        
        assert_eq!(decoded.limit, cursor.limit);
        assert_eq!(decoded.last_pubkey, cursor.last_pubkey);
    }
    
    #[test]
    fn test_pagination_helper_basic() {
        let helper = PaginationHelper::new(10, 100);
        
        let results: Vec<Pubkey> = (0..25).map(|_| Pubkey::new_unique()).collect();
        
        // First page
        let page1 = helper.paginate(&results, None);
        assert_eq!(page1.results.len(), 10);
        assert!(page1.next_cursor.is_some());
        assert!(!page1.is_last_page);
        
        // Second page
        let cursor = page1.next_cursor.as_ref().unwrap();
        let page2 = helper.paginate(&results, Some(cursor));
        assert_eq!(page2.results.len(), 10);
        assert!(page2.next_cursor.is_some());
        assert!(!page2.is_last_page);
        
        // Last page
        let cursor = page2.next_cursor.as_ref().unwrap();
        let page3 = helper.paginate(&results, Some(cursor));
        assert_eq!(page3.results.len(), 5);
        assert!(page3.next_cursor.is_none());
        assert!(page3.is_last_page);
    }
}
