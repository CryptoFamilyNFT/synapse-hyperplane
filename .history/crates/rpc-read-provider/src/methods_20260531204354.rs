//! RPC Method Handlers
//!
//! Implements getAccountInfo, getMultipleAccounts, and other read methods

use serde_json::json;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use crate::server::RpcError;

/// Handle getAccountInfo method
pub fn handle_get_account_info(
    params: &serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    let params = params.as_array().ok_or_else(|| {
        RpcError::InvalidParams("Expected array".to_string())
    })?;

    if params.is_empty() {
        return Err(RpcError::InvalidParams("Missing pubkey".to_string()));
    }

    // Parse pubkey
    let pubkey_str = params[0]
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams("Pubkey must be string".to_string()))?;
    
    let pubkey = Pubkey::from_str(pubkey_str)
        .map_err(|e| RpcError::InvalidParams(format!("Invalid pubkey: {}", e)))?;

    // Parse options (if present)
    let (_encoding, _commitment, _with_context) = if params.len() > 1 {
        if let Some(opts) = params[1].as_object() {
            let enc = opts.get("encoding").and_then(|v| v.as_str()).unwrap_or("base64");
            let comm = opts.get("commitment").and_then(|v| v.as_str()).unwrap_or("processed");
            let wc = opts.get("withContext").and_then(|v| v.as_bool()).unwrap_or(true);
            (enc, comm, wc)
        } else {
            ("base64", "processed", true)
        }
    } else {
        ("base64", "processed", true)
    };

    // TODO: Actually fetch from Hyperplane engine
    // For now, return placeholder
    let null_value: Option<serde_json::Value> = None;
    Ok(json!({
        "context": {
            "slot": 0,
            "apiVersion": "synapse-hyperplane/0.1.0"
        },
        "value": null_value
    }))
}

/// Handle getMultipleAccounts method
pub fn handle_get_multiple_accounts(
    params: &serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    let params = params.as_array().ok_or_else(|| {
        RpcError::InvalidParams("Expected array".to_string())
    })?;

    if params.is_empty() {
        return Err(RpcError::InvalidParams("Missing pubkeys".to_string()));
    }

    let pubkeys_value = &params[0];
    let pubkeys_arr = pubkeys_value
        .as_array()
        .ok_or_else(|| RpcError::InvalidParams("Pubkeys must be array".to_string()))?;

    // Parse pubkeys
    let mut pubkeys = Vec::new();
    for pk_val in pubkeys_arr {
        let pk_str = pk_val
            .as_str()
            .ok_or_else(|| RpcError::InvalidParams("Pubkey must be string".to_string()))?;
        
        let pubkey = Pubkey::from_str(pk_str)
            .map_err(|e| RpcError::InvalidParams(format!("Invalid pubkey {}: {}", pk_str, e)))?;
        
        pubkeys.push(pubkey);
    }

    // Limit batch size
    if pubkeys.len() > 1000 {
        return Err(RpcError::InvalidParams(
            "Batch size exceeds limit of 1000".to_string()
        ));
    }

    // TODO: Actually fetch from Hyperplane engine
    // For now, return placeholder
    let null_value: Option<serde_json::Value> = None;
    let nulls: Vec<Option<serde_json::Value>> = vec![null_value; pubkeys.len()];
    Ok(json!({
        "context": {
            "slot": 0,
            "apiVersion": "synapse-hyperplane/0.1.0"
        },
        "value": nulls
    }))
}

/// Handle health check
pub fn handle_health() -> hyper::Response<http_body_util::Full<bytes::Bytes>> {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::{Response, StatusCode};

    let body = json!({
        "status": "ok",
        "version": "synapse-hyperplane/0.1.0"
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string().into_bytes())))
        .unwrap()
}

/// Handle metrics endpoint
pub fn handle_metrics(
    state: &crate::server::RpcServerState,
) -> hyper::Response<http_body_util::Full<bytes::Bytes>> {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::{Response, StatusCode};

    let request_count = state.get_request_count();
    
    let metrics = format!(
        "# HELP synapse_requests_total Total RPC requests\n\
         # TYPE synapse_requests_total counter\n\
         synapse_requests_total {}\n",
        request_count
    );

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(Full::new(Bytes::from(metrics.into_bytes())))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_account_info_params() {
        let params = json!(["11111111111111111111111111111111"]);
        let result = handle_get_account_info(&params);
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_account_info_invalid_pubkey() {
        let params = json!(["invalid_pubkey"]);
        let result = handle_get_account_info(&params);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_get_multiple_accounts_batch() {
        let params = json!([
            ["11111111111111111111111111111111", "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"]
        ]);
        let result = handle_get_multiple_accounts(&params);
        
        assert!(result.is_ok());
    }
}
