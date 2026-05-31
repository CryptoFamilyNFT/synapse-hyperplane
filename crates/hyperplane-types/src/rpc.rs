//! RPC types and request/response structures
//!
//! Defines the RPC API surface for Synapse Hyperplane,
//! including getProgramAccountsV2 with pagination and streaming support.

use serde::{Deserialize, Serialize};
use solana_sdk::{clock::Slot, pubkey::Pubkey};

use crate::account::AccountView;
use crate::slot::CommitmentLevel;

/// RPC request context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcContext {
    /// Commitment level for the query
    pub commitment: CommitmentLevel,
    /// Encoding format for account data
    pub encoding: AccountEncoding,
    /// Whether to include context (slot info) in response
    pub with_context: bool,
    /// Max accounts to return (for pagination)
    pub limit: Option<usize>,
    /// Cursor for pagination
    pub cursor: Option<String>,
    /// Sort order for results
    pub sort: SortOrder,
}

impl Default for RpcContext {
    fn default() -> Self {
        Self {
            commitment: CommitmentLevel::Processed,
            encoding: AccountEncoding::Base64,
            with_context: true,
            limit: Some(1000),
            cursor: None,
            sort: SortOrder::Pubkey,
        }
    }
}

/// Account encoding formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountEncoding {
    /// Base58 encoding (legacy, slow)
    Base58,
    /// Base64 encoding (standard)
    Base64,
    /// Base64 + zero padding for performance
    Base64Zstd,
    /// Binary (raw bytes, for internal use)
    Binary,
    /// JSON parsed (for known IDLs)
    JsonParsed,
}

impl Default for AccountEncoding {
    fn default() -> Self {
        Self::Base64
    }
}

/// Sort order for paginated results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    /// Sort by pubkey (default)
    Pubkey,
    /// Sort by lamports
    Lamports,
    /// Sort by data size
    DataSize,
    /// Sort by slot (most recent first)
    Slot,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Pubkey
    }
}

/// Filter for getProgramAccounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFilter {
    /// Filter by data size
    pub data_size: Option<u64>,
    /// Memcmp filter (offset + bytes)
    pub memcmp: Option<MemcmpFilter>,
    /// Token mint filter
    pub mint: Option<Pubkey>,
    /// Token owner filter
    pub token_owner: Option<Pubkey>,
}

/// Memcmp filter for account data matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemcmpFilter {
    /// Offset in account data
    pub offset: usize,
    /// Bytes to match (hex string)
    pub bytes: String,
    /// Optional encoding hint
    pub encoding: Option<String>,
}

/// getProgramAccountsV2 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProgramAccountsV2Request {
    /// Program ID to query
    pub program_id: Pubkey,
    /// Filters to apply
    pub filters: Vec<AccountFilter>,
    /// Pagination cursor
    pub cursor: Option<String>,
    /// Max results per page
    pub limit: usize,
    /// Account encoding
    pub encoding: AccountEncoding,
    /// Commitment level
    pub commitment: CommitmentLevel,
    /// Sort order
    pub sort: SortOrder,
    /// Include context in response
    pub with_context: bool,
}

impl Default for GetProgramAccountsV2Request {
    fn default() -> Self {
        Self {
            program_id: Pubkey::default(),
            filters: Vec::new(),
            cursor: None,
            limit: 1000,
            encoding: AccountEncoding::Base64,
            commitment: CommitmentLevel::Processed,
            sort: SortOrder::Pubkey,
            with_context: true,
        }
    }
}

/// Account info in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAccountInfo {
    /// Account pubkey (base58)
    pub pubkey: String,
    /// Account lamports
    pub lamports: u64,
    /// Account data (encoded)
    pub data: RpcAccountData,
    /// Account owner
    pub owner: String,
    /// Whether account is executable
    pub executable: bool,
    /// Rent epoch
    pub rent_epoch: u64,
    /// Slot of last modification
    pub slot: Slot,
}

/// Encoded account data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAccountData {
    /// Data encoding
    pub encoding: AccountEncoding,
    /// Data as string (base64, base58, etc.)
    pub data: String,
    /// Data size in bytes
    pub data_size: usize,
}

/// getProgramAccountsV2 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProgramAccountsV2Response {
    /// Context information
    pub context: RpcResponseContext,
    /// Paginated accounts
    pub accounts: Vec<RpcAccountInfo>,
    /// Cursor for next page
    pub cursor: Option<String>,
    /// Whether more results exist
    pub has_more: bool,
    /// Total candidate count (from bitmap)
    pub total_candidates: u64,
    /// Query execution time in ms
    pub execution_ms: u128,
}

/// Response context with slot info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponseContext {
    pub slot: Slot,
    pub api_version: String,
}

impl RpcResponseContext {
    pub fn new(slot: Slot) -> Self {
        Self {
            slot,
            api_version: "synapse-hyperplane/0.1.0".to_string(),
        }
    }
}

/// getAccountInfo response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAccountInfoResponse {
    pub context: RpcResponseContext,
    pub value: Option<RpcAccountInfo>,
    pub cache_hit: bool,
    pub storage_type: Option<String>,
    pub execution_ms: u128,
}

/// getMultipleAccounts response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMultipleAccountsResponse {
    pub context: RpcResponseContext,
    pub value: Vec<RpcAccountInfo>,
    pub missing_pubkeys: Vec<String>,
    pub cache_hits: usize,
    pub execution_ms: u128,
}

/// Convert AccountView to RpcAccountInfo
pub fn account_view_to_rpc(
    view: &AccountView,
    encoding: AccountEncoding,
) -> RpcAccountInfo {
    let (data_str, data_size) = match encoding {
        AccountEncoding::Base64 => {
            use base64::{Engine, engine::general_purpose::STANDARD};
            let encoded = STANDARD.encode(view.data.as_slice());
            (encoded, view.data.len())
        }
        AccountEncoding::Base58 => {
            let encoded = solana_sdk::bs58::encode(view.data.as_slice()).into_string();
            (encoded, view.data.len())
        }
        _ => {
            // Default to base64
            use base64::{Engine, engine::general_purpose::STANDARD};
            let encoded = STANDARD.encode(view.data.as_slice());
            (encoded, view.data.len())
        }
    };

    RpcAccountInfo {
        pubkey: view.pubkey.to_string(),
        lamports: view.lamports,
        data: RpcAccountData {
            encoding,
            data: data_str,
            data_size,
        },
        owner: view.owner.to_string(),
        executable: view.executable,
        rent_epoch: view.rent_epoch,
        slot: view.slot,
    }
}

/// Query statistics for adaptive indexing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// RPC method name
    pub method: String,
    /// Program ID (if applicable)
    pub program_id: Option<Pubkey>,
    /// Filters applied
    pub filters: Vec<String>,
    /// Number of candidates from bitmap
    pub candidate_count: u64,
    /// Number of results returned
    pub result_count: usize,
    /// Execution time in ms
    pub execution_ms: u128,
    /// Cache hit
    pub cache_hit: bool,
    /// Slot watermark used
    pub slot_watermark: Slot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_context_defaults() {
        let ctx = RpcContext::default();
        assert_eq!(ctx.commitment, CommitmentLevel::Processed);
        assert_eq!(ctx.encoding, AccountEncoding::Base64);
        assert_eq!(ctx.limit, Some(1000));
        assert!(ctx.with_context);
    }

    #[test]
    fn test_filter_serialization() {
        let filter = AccountFilter {
            data_size: Some(165),
            memcmp: Some(MemcmpFilter {
                offset: 32,
                bytes: "0x1234567890abcdef".to_string(),
                encoding: Some("hex".to_string()),
            }),
            mint: None,
            token_owner: None,
        };

        let json = serde_json::to_string(&filter).unwrap();
        let deserialized: AccountFilter = serde_json::from_str(&json).unwrap();
        
        assert_eq!(deserialized.data_size, Some(165));
        assert!(deserialized.memcmp.is_some());
    }
}
