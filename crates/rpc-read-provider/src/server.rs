//! RPC Server implementation
//!
//! Hyper-based HTTP server with JSON-RPC 2.0 support

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use serde_json::json;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpListener;
use tracing::{error, info, warn};

use crate::methods::{handle_get_account_info, handle_get_multiple_accounts, handle_health, handle_metrics};
use crate::middleware::{RateLimiter, RequestLogger};

/// RPC Server configuration
#[derive(Debug, Clone)]
pub struct RpcServerConfig {
    /// Bind address
    pub bind: SocketAddr,
    /// Number of worker threads
    pub workers: usize,
    /// Max concurrent requests
    pub max_concurrent: usize,
    /// Rate limit per IP (requests/second)
    pub rate_limit_per_ip: usize,
    /// Enable CORS
    pub enable_cors: bool,
    /// Health check endpoint
    pub health_endpoint: bool,
    /// Metrics endpoint
    pub metrics_endpoint: bool,
}

impl Default for RpcServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8898".parse().unwrap(),
            workers: 32,
            max_concurrent: 1000,
            rate_limit_per_ip: 100,
            enable_cors: true,
            health_endpoint: true,
            metrics_endpoint: true,
        }
    }
}

/// RPC Server state
pub struct RpcServerState {
    /// Request counter
    request_count: AtomicU64,
    /// Rate limiter
    rate_limiter: RateLimiter,
    /// Method handlers
    handlers: RwLock<HashMap<String, Box<dyn MethodHandler + Send + Sync>>>,
}

impl RpcServerState {
    pub fn new(config: &RpcServerConfig) -> Self {
        Self {
            request_count: AtomicU64::new(0),
            rate_limiter: RateLimiter::new(config.rate_limit_per_ip),
            handlers: RwLock::new(HashMap::new()),
        }
    }

    pub fn increment_request_count(&self) -> u64 {
        self.request_count.fetch_add(1, Ordering::Relaxed)
    }

    pub fn get_request_count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }
}

/// Method handler trait
pub trait MethodHandler {
    fn handle(&self, params: serde_json::Value) -> Result<serde_json::Value, RpcError>;
}

/// RPC Error
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    
    #[error("Invalid params: {0}")]
    InvalidParams(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Rate limited")]
    RateLimited,
    
    #[error("Parse error: {0}")]
    ParseError(String),
}

impl RpcError {
    pub fn to_json_rpc_error(&self, id: Option<serde_json::Value>) -> serde_json::Value {
        let (code, message) = match self {
            Self::MethodNotFound(_) => (-32601, "Method not found"),
            Self::InvalidParams(_) => (-32602, "Invalid params"),
            Self::InternalError(_) => (-32603, "Internal error"),
            Self::RateLimited => (-32005, "Rate limited"),
            Self::ParseError(_) => (-32700, "Parse error"),
        };
        
        json!({
            "jsonrpc": "2.0",
            "error": {
                "code": code,
                "message": message,
                "data": self.to_string()
            },
            "id": id.unwrap_or(json!(null))
        })
    }
}

/// RPC Request
#[derive(Debug, serde::Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
}

/// RPC Response
#[derive(Debug, serde::Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// RPC Server
pub struct RpcServer {
    config: RpcServerConfig,
    state: Arc<RpcServerState>,
}

impl RpcServer {
    pub fn new(config: RpcServerConfig) -> Self {
        let state = Arc::new(RpcServerState::new(&config));
        
        Self { config, state }
    }

    /// Start server
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.config.bind;
        let listener = TcpListener::bind(addr).await?;
        
        info!("RPC server listening on {}", addr);
        
        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let io = TokioIo::new(stream);
            
            let state = self.state.clone();
            let config = self.config.clone();
            
            tokio::task::spawn(async move {
                let service = service_fn(move |req| {
                    handle_request(req, state.clone(), peer_addr, config.clone())
                });
                
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                {
                    warn!("Error serving connection: {:?}", err);
                }
            });
        }
    }

    /// Get server state
    pub fn state(&self) -> Arc<RpcServerState> {
        self.state.clone()
    }
}

/// Handle HTTP request
async fn handle_request(
    req: Request<Incoming>,
    state: Arc<RpcServerState>,
    peer_addr: SocketAddr,
    config: RpcServerConfig,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    // Extract path for special endpoints
    let path = req.uri().path().to_string();
    
    // Handle special endpoints (GET methods)
    if req.method() == hyper::Method::GET {
        if path == "/health" {
            return Ok(handle_health());
        }
        
        if path == "/metrics" {
            return Ok(handle_metrics(&state));
        }
        
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not found")))
            .unwrap());
    }
    
    // All other methods must be POST
    if req.method() != hyper::Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Full::new(Bytes::from("Method not allowed")))
            .unwrap());
    }

    // Rate limiting
    let ip = peer_addr.ip().to_string();
    if !state.rate_limiter.allow(&ip) {
        let error = RpcError::RateLimited.to_json_rpc_error(None);
        let error_bytes = serde_json::to_vec(&error).unwrap_or_default();
        return Ok(json_response(&error_bytes, StatusCode::TOO_MANY_REQUESTS));
    }

    // Read body
    let body_bytes = req.collect().await?.to_bytes();
    
    // Parse request
    let rpc_req: RpcRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            let error = RpcError::ParseError(e.to_string()).to_json_rpc_error(None);
            let error_bytes = serde_json::to_vec(&error).unwrap_or_default();
            return Ok(json_response(&error_bytes, StatusCode::BAD_REQUEST));
        }
    };

    // Route RPC method
    let result = match rpc_req.method.as_str() {
        "getAccountInfo" => handle_get_account_info(&rpc_req.params),
        "getMultipleAccounts" => handle_get_multiple_accounts(&rpc_req.params),
        _ => Err(RpcError::MethodNotFound(rpc_req.method)),
    };

    let response = match result {
        Ok(value) => RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(value),
            error: None,
            id: rpc_req.id,
        },
        Err(e) => {
            let error_response = e.to_json_rpc_error(rpc_req.id.clone());
            let error_bytes = serde_json::to_vec(&error_response).unwrap_or_default();
            return Ok(json_response(&error_bytes, StatusCode::OK));
        }
    };

    let body = serde_json::to_vec(&response).unwrap_or_default();
    Ok(json_response(&body, StatusCode::OK))
}

/// Create JSON response
fn json_response(body: &[u8], status: StatusCode) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Full::new(Bytes::from(body.to_vec())))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_error_serialization() {
        let error = RpcError::MethodNotFound("test".to_string());
        let json = error.to_json_rpc_error(Some(json!(1)));
        
        assert_eq!(json["error"]["code"], -32601);
        assert_eq!(json["error"]["message"], "Method not found");
    }

    #[test]
    fn test_request_parsing() {
        let json = r#"{"jsonrpc":"2.0","method":"getAccountInfo","params":["Pubkey123"],"id":1}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        
        assert_eq!(req.method, "getAccountInfo");
        assert_eq!(req.id.unwrap(), 1);
    }
}
