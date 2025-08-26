//! Tendermint ABCI Server for Sedly

use crate::abci::{SedlyApp, ConsensusError};
use tendermint_abci::{Application, Server, ServerBuilder};
use tokio::net::TcpListener;
use std::sync::Arc;

/// Configuration for consensus server
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// ABCI server bind address
    pub abci_addr: String,
    /// Database path for blockchain storage
    pub db_path: String,
    /// Maximum number of connections
    pub max_connections: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            abci_addr: "127.0.0.1:26658".to_string(),
            db_path: "./blockchain_data".to_string(),
            max_connections: 100,
        }
    }
}

/// Consensus server managing ABCI application
pub struct ConsensusServer {
    /// Server configuration
    config: ServerConfig,
    /// ABCI application
    app: Arc<SedlyApp>,
}

impl ConsensusServer {
    /// Create new consensus server
    pub fn new(config: ServerConfig) -> Result<Self, ConsensusError> {
        let app = Arc::new(SedlyApp::new(&config.db_path)?);

        Ok(Self {
            config,
            app,
        })
    }

    /// Start the ABCI server
    pub async fn start(&self) -> Result<(), ConsensusError> {
        log::info!("Starting Sedly consensus server on {}", self.config.abci_addr);

        // Create TCP listener
        let listener = TcpListener::bind(&self.config.abci_addr)
            .await
            .map_err(|e| ConsensusError::ConsensusError(format!("Failed to bind ABCI server: {}", e)))?;

        log::info!("ABCI server listening on {}", self.config.abci_addr);

        // Create server with our application
        let server = ServerBuilder::default()
            .build(self.app.clone())
            .map_err(|e| ConsensusError::ConsensusError(format!("Failed to create server: {}", e)))?;

        // Run server
        server
            .listen(listener)
            .await
            .map_err(|e| ConsensusError::ConsensusError(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Get reference to the ABCI application
    pub fn app(&self) -> Arc<SedlyApp> {
        Arc::clone(&self.app)
    }

    /// Get server configuration
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
}

/// Builder for consensus server
pub struct ConsensusServerBuilder {
    config: ServerConfig,
}

impl ConsensusServerBuilder {
    /// Create new builder with default config
    pub fn new() -> Self {
        Self {
            config: ServerConfig::default(),
        }
    }

    /// Set ABCI bind address
    pub fn abci_addr<S: Into<String>>(mut self, addr: S) -> Self {
        self.config.abci_addr = addr.into();
        self
    }

    /// Set database path
    pub fn db_path<S: Into<String>>(mut self, path: S) -> Self {
        self.config.db_path = path.into();
        self
    }

    /// Set maximum connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.config.max_connections = max;
        self
    }

    /// Build the consensus server
    pub fn build(self) -> Result<ConsensusServer, ConsensusError> {
        ConsensusServer::new(self.config)
    }
}

impl Default for ConsensusServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Start a basic consensus server with default configuration
pub async fn start_server(db_path: &str) -> Result<(), ConsensusError> {
    let server = ConsensusServerBuilder::new()
        .db_path(db_path)
        .build()?;

    server.start().await
}

/// Start consensus server with custom configuration
pub async fn start_server_with_config(config: ServerConfig) -> Result<(), ConsensusError> {
    let server = ConsensusServer::new(config)?;
    server.start().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_server_config() {
        let config = ServerConfig {
            abci_addr: "127.0.0.1:9999".to_string(),
            db_path: "/tmp/test".to_string(),
            max_connections: 50,
        };

        assert_eq!(config.abci_addr, "127.0.0.1:9999");
        assert_eq!(config.db_path, "/tmp/test");
        assert_eq!(config.max_connections, 50);
    }

    #[test]
    fn test_server_builder() {
        let temp_dir = TempDir::new().unwrap();

        let server = ConsensusServerBuilder::new()
            .abci_addr("127.0.0.1:8888")
            .db_path(temp_dir.path().to_str().unwrap())
            .max_connections(25)
            .build()
            .unwrap();

        assert_eq!(server.config().abci_addr, "127.0.0.1:8888");
        assert_eq!(server.config().max_connections, 25);
    }

    #[test]
    fn test_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ServerConfig {
            abci_addr: "127.0.0.1:26658".to_string(),
            db_path: temp_dir.path().to_str().unwrap().to_string(),
            max_connections: 100,
        };

        let server = ConsensusServer::new(config);
        assert!(server.is_ok());
    }
}