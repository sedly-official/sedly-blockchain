//! Tendermint ABCI Application implementation for Sedly

use sedly_core::{
    Block, Transaction, BlockchainDB, ChainMetadata, DifficultyAdjuster,
    Miner, INITIAL_BLOCK_REWARD, HALVING_INTERVAL
};
use tendermint_abci::{
    Application, RequestBeginBlock, RequestCheckTx, RequestCommit, RequestDeliverTx,
    RequestEndBlock, RequestInfo, RequestInitChain, RequestQuery,
    ResponseBeginBlock, ResponseCheckTx, ResponseCommit, ResponseDeliverTx,
    ResponseEndBlock, ResponseInfo, ResponseInitChain, ResponseQuery,
    ConsensusParams, ValidatorUpdate,
};
use tendermint::abci::{Code, Event, EventAttribute};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Sedly ABCI Application
pub struct SedlyApp {
    /// Blockchain database
    db: Arc<BlockchainDB>,
    /// Current block being built
    current_block: Arc<Mutex<Option<BlockBuilder>>>,
    /// Transaction pool for pending transactions
    mempool: Arc<Mutex<HashMap<[u8; 32], Transaction>>>,
    /// Difficulty adjuster
    difficulty_adjuster: DifficultyAdjuster,
    /// Current chain state
    chain_state: Arc<Mutex<ChainState>>,
}

/// Block being constructed during consensus
#[derive(Debug, Clone)]
struct BlockBuilder {
    /// Transactions included in this block
    transactions: Vec<Transaction>,
    /// Block height
    height: u64,
    /// Previous block hash
    previous_hash: [u8; 32],
    /// Timestamp when block building started
    timestamp: u64,
    /// Current difficulty bits
    bits: u32,
}

/// Current state of the blockchain
#[derive(Debug, Clone)]
struct ChainState {
    /// Current height
    height: u64,
    /// Best block hash
    best_block_hash: [u8; 32],
    /// Total transactions processed
    total_transactions: u64,
    /// Current difficulty bits
    current_bits: u32,
}

/// Transaction check result
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TxCheckResult {
    /// Whether transaction is valid
    valid: bool,
    /// Error message if invalid
    error: Option<String>,
    /// Gas used (for future fee calculation)
    gas_used: u64,
}

impl SedlyApp {
    /// Create new ABCI application
    pub fn new(db_path: &str) -> Result<Self, ConsensusError> {
        let db = Arc::new(
            BlockchainDB::open(db_path)
                .map_err(|e| ConsensusError::DatabaseError(e.to_string()))?
        );

        // Initialize with genesis if empty
        let metadata = db.get_metadata()
            .map_err(|e| ConsensusError::DatabaseError(e.to_string()))?;

        let chain_state = if metadata.height == 0 {
            // Initialize with genesis
            let genesis = Block::genesis();
            db.initialize_with_genesis(&genesis)
                .map_err(|e| ConsensusError::DatabaseError(e.to_string()))?;

            ChainState {
                height: 0,
                best_block_hash: genesis.hash(),
                total_transactions: 1, // Genesis transaction
                current_bits: DifficultyAdjuster::genesis_difficulty(),
            }
        } else {
            ChainState {
                height: metadata.height,
                best_block_hash: metadata.best_block_hash,
                total_transactions: 0, // Will be calculated if needed
                current_bits: DifficultyAdjuster::genesis_difficulty(), // Will be updated
            }
        };

        Ok(Self {
            db,
            current_block: Arc::new(Mutex::new(None)),
            mempool: Arc::new(Mutex::new(HashMap::new())),
            difficulty_adjuster: DifficultyAdjuster::new(),
            chain_state: Arc::new(Mutex::new(chain_state)),
        })
    }

    /// Validate transaction against current state
    fn check_transaction(&self, tx: &Transaction) -> TxCheckResult {
        // Basic validation
        if !tx.is_valid() {
            return TxCheckResult {
                valid: false,
                error: Some("Invalid transaction structure".to_string()),
                gas_used: 0,
            };
        }

        // Check if coinbase (only allowed in block building)
        if tx.is_coinbase() {
            return TxCheckResult {
                valid: false,
                error: Some("Coinbase transactions not allowed in mempool".to_string()),
                gas_used: 0,
            };
        }

        // Verify inputs exist and are spendable
        let chain_state = self.chain_state.lock().unwrap();
        for input in &tx.inputs {
            match self.db.is_utxo_spendable(&input.previous_output, chain_state.height) {
                Ok(true) => continue,
                Ok(false) => {
                    return TxCheckResult {
                        valid: false,
                        error: Some("UTXO not found or not spendable".to_string()),
                        gas_used: 0,
                    };
                }
                Err(e) => {
                    return TxCheckResult {
                        valid: false,
                        error: Some(format!("Database error: {}", e)),
                        gas_used: 0,
                    };
                }
            }
        }

        // TODO: Verify signatures
        // TODO: Calculate fees and gas

        TxCheckResult {
            valid: true,
            error: None,
            gas_used: tx.size() as u64, // Simple gas model
        }
    }

    /// Calculate current block reward
    fn calculate_block_reward(&self, height: u64) -> u64 {
        let halvings = height / HALVING_INTERVAL;
        if halvings >= 64 {
            0 // No more rewards after 64 halvings
        } else {
            INITIAL_BLOCK_REWARD >> halvings
        }
    }

    /// Create coinbase transaction for block
    fn create_coinbase(&self, height: u64, beneficiary: &[u8]) -> Transaction {
        let reward = self.calculate_block_reward(height);
        Transaction::coinbase(beneficiary, height, reward)
    }

    /// Update difficulty if needed
    fn update_difficulty(&self, height: u64) -> u32 {
        if height % sedly_core::DIFFICULTY_ADJUSTMENT_INTERVAL == 0 && height > 0 {
            // Get recent blocks for difficulty calculation
            let start_height = height.saturating_sub(sedly_core::DIFFICULTY_ADJUSTMENT_INTERVAL);
            let mut recent_blocks = Vec::new();

            for h in start_height..height {
                if let Ok(Some(block)) = self.db.get_block_by_height(h) {
                    recent_blocks.push(block);
                }
            }

            if recent_blocks.len() == sedly_core::DIFFICULTY_ADJUSTMENT_INTERVAL as usize {
                let current_state = self.chain_state.lock().unwrap();
                match self.difficulty_adjuster.calculate_next_difficulty(&recent_blocks, current_state.current_bits) {
                    Ok(adjustment) => {
                        log::info!("Difficulty adjustment: {}", adjustment.format_adjustment());
                        return adjustment.new_bits;
                    }
                    Err(e) => {
                        log::warn!("Failed to calculate difficulty adjustment: {}", e);
                    }
                }
            }
        }

        // Return current difficulty
        self.chain_state.lock().unwrap().current_bits
    }
}

impl Application for SedlyApp {
    /// Get application info
    fn info(&self, _request: RequestInfo) -> ResponseInfo {
        let chain_state = self.chain_state.lock().unwrap();

        ResponseInfo {
            data: "Sedly Blockchain".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            app_version: 1,
            last_block_height: chain_state.height as i64,
            last_block_app_hash: chain_state.best_block_hash.to_vec().into(),
        }
    }

    /// Initialize blockchain with genesis
    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        log::info!("Initializing chain with genesis");

        // Chain should already be initialized in constructor
        let chain_state = self.chain_state.lock().unwrap();

        ResponseInitChain {
            consensus_params: request.consensus_params,
            validators: vec![], // No validators for PoW
            app_hash: chain_state.best_block_hash.to_vec().into(),
        }
    }

    /// Check transaction validity
    fn check_tx(&self, request: RequestCheckTx) -> ResponseCheckTx {
        match bincode::deserialize::<Transaction>(&request.tx) {
            Ok(tx) => {
                let result = self.check_transaction(&tx);

                if result.valid {
                    ResponseCheckTx {
                        code: Code::Ok,
                        data: vec![].into(),
                        log: "Transaction valid".to_string(),
                        info: "".to_string(),
                        gas_wanted: result.gas_used as i64,
                        gas_used: result.gas_used as i64,
                        events: vec![],
                        codespace: "".to_string(),
                        mempool_error: "".to_string(),
                        priority: 0,
                        sender: "".to_string(),
                    }
                } else {
                    ResponseCheckTx {
                        code: Code::Err(1),
                        data: vec![].into(),
                        log: result.error.unwrap_or("Invalid transaction".to_string()),
                        info: "".to_string(),
                        gas_wanted: 0,
                        gas_used: 0,
                        events: vec![],
                        codespace: "sedly".to_string(),
                        mempool_error: "".to_string(),
                        priority: 0,
                        sender: "".to_string(),
                    }
                }
            }
            Err(e) => {
                ResponseCheckTx {
                    code: Code::Err(2),
                    data: vec![].into(),
                    log: format!("Failed to decode transaction: {}", e),
                    info: "".to_string(),
                    gas_wanted: 0,
                    gas_used: 0,
                    events: vec![],
                    codespace: "sedly".to_string(),
                    mempool_error: "".to_string(),
                    priority: 0,
                    sender: "".to_string(),
                }
            }
        }
    }

    /// Begin new block construction
    fn begin_block(&self, request: RequestBeginBlock) -> ResponseBeginBlock {
        let height = request.header.height.value();
        log::info!("Beginning block {}", height);

        let chain_state = self.chain_state.lock().unwrap();
        let previous_hash = chain_state.best_block_hash;
        drop(chain_state);

        // Update difficulty
        let new_bits = self.update_difficulty(height as u64);

        // Create block builder
        let block_builder = BlockBuilder {
            transactions: Vec::new(),
            height: height as u64,
            previous_hash,
            timestamp: request.header.time.seconds as u64,
            bits: new_bits,
        };

        // Add coinbase transaction
        // TODO: Get proper beneficiary from validator/miner
        let coinbase = self.create_coinbase(height as u64, b"sedly_validator");
        let mut builder = block_builder;
        builder.transactions.push(coinbase);

        *self.current_block.lock().unwrap() = Some(builder);

        ResponseBeginBlock {
            events: vec![
                Event {
                    type_str: "begin_block".to_string(),
                    attributes: vec![
                        EventAttribute {
                            key: "height".to_string(),
                            value: height.to_string(),
                            index: false,
                        },
                        EventAttribute {
                            key: "difficulty".to_string(),
                            value: format!("0x{:08x}", new_bits),
                            index: false,
                        },
                    ],
                }
            ],
        }
    }

    /// Deliver transaction to be included in block
    fn deliver_tx(&self, request: RequestDeliverTx) -> ResponseDeliverTx {
        match bincode::deserialize::<Transaction>(&request.tx) {
            Ok(tx) => {
                let result = self.check_transaction(&tx);

                if result.valid {
                    // Add to current block
                    if let Some(ref mut builder) = self.current_block.lock().unwrap().as_mut() {
                        builder.transactions.push(tx.clone());

                        ResponseDeliverTx {
                            code: Code::Ok,
                            data: tx.hash().to_vec().into(),
                            log: "Transaction delivered".to_string(),
                            info: "".to_string(),
                            gas_wanted: result.gas_used as i64,
                            gas_used: result.gas_used as i64,
                            events: vec![
                                Event {
                                    type_str: "deliver_tx".to_string(),
                                    attributes: vec![
                                        EventAttribute {
                                            key: "txhash".to_string(),
                                            value: hex::encode(tx.hash()),
                                            index: true,
                                        },
                                    ],
                                }
                            ],
                            codespace: "".to_string(),
                        }
                    } else {
                        ResponseDeliverTx {
                            code: Code::Err(3),
                            data: vec![].into(),
                            log: "No block being built".to_string(),
                            info: "".to_string(),
                            gas_wanted: 0,
                            gas_used: 0,
                            events: vec![],
                            codespace: "sedly".to_string(),
                        }
                    }
                } else {
                    ResponseDeliverTx {
                        code: Code::Err(1),
                        data: vec![].into(),
                        log: result.error.unwrap_or("Invalid transaction".to_string()),
                        info: "".to_string(),
                        gas_wanted: 0,
                        gas_used: 0,
                        events: vec![],
                        codespace: "sedly".to_string(),
                    }
                }
            }
            Err(e) => {
                ResponseDeliverTx {
                    code: Code::Err(2),
                    data: vec![].into(),
                    log: format!("Failed to decode transaction: {}", e),
                    info: "".to_string(),
                    gas_wanted: 0,
                    gas_used: 0,
                    events: vec![],
                    codespace: "sedly".to_string(),
                }
            }
        }
    }

    /// End block construction
    fn end_block(&self, request: RequestEndBlock) -> ResponseEndBlock {
        let height = request.height;
        log::info!("Ending block {}", height);

        ResponseEndBlock {
            validator_updates: vec![], // No validator updates for PoW
            consensus_param_updates: None,
            events: vec![
                Event {
                    type_str: "end_block".to_string(),
                    attributes: vec![
                        EventAttribute {
                            key: "height".to_string(),
                            value: height.to_string(),
                            index: false,
                        },
                    ],
                }
            ],
        }
    }

    /// Commit block to blockchain
    fn commit(&self, _request: RequestCommit) -> ResponseCommit {
        if let Some(builder) = self.current_block.lock().unwrap().take() {
            // Create final block
            let block = Block::new(
                builder.previous_hash,
                builder.transactions,
                builder.bits,
                builder.height,
            );

            // Store block in database
            match self.db.store_block(&block) {
                Ok(()) => {
                    // Update chain state
                    let mut chain_state = self.chain_state.lock().unwrap();
                    chain_state.height = builder.height;
                    chain_state.best_block_hash = block.hash();
                    chain_state.current_bits = builder.bits;
                    chain_state.total_transactions += block.transactions.len() as u64;

                    log::info!("Committed block {} with {} transactions",
                              builder.height, block.transactions.len());

                    ResponseCommit {
                        data: block.hash().to_vec().into(),
                        retain_height: 0, // Keep all blocks
                    }
                }
                Err(e) => {
                    log::error!("Failed to store block: {}", e);
                    ResponseCommit {
                        data: vec![].into(),
                        retain_height: 0,
                    }
                }
            }
        } else {
            log::error!("No block to commit");
            ResponseCommit {
                data: vec![].into(),
                retain_height: 0,
            }
        }
    }

    /// Handle queries
    fn query(&self, request: RequestQuery) -> ResponseQuery {
        let path_parts: Vec<&str> = request.path.split('/').collect();

        match path_parts.as_slice() {
            ["block", height_str] => {
                if let Ok(height) = height_str.parse::<u64>() {
                    match self.db.get_block_by_height(height) {
                        Ok(Some(block)) => {
                            match bincode::serialize(&block) {
                                Ok(data) => ResponseQuery {
                                    code: Code::Ok,
                                    log: "Block found".to_string(),
                                    info: "".to_string(),
                                    index: 0,
                                    key: request.data.to_vec().into(),
                                    value: data.into(),
                                    proof_ops: None,
                                    height: height as i64,
                                    codespace: "".to_string(),
                                },
                                Err(e) => ResponseQuery {
                                    code: Code::Err(1),
                                    log: format!("Serialization error: {}", e),
                                    info: "".to_string(),
                                    index: 0,
                                    key: vec![].into(),
                                    value: vec![].into(),
                                    proof_ops: None,
                                    height: 0,
                                    codespace: "sedly".to_string(),
                                }
                            }
                        }
                        Ok(None) => ResponseQuery {
                            code: Code::Err(2),
                            log: "Block not found".to_string(),
                            info: "".to_string(),
                            index: 0,
                            key: vec![].into(),
                            value: vec![].into(),
                            proof_ops: None,
                            height: 0,
                            codespace: "sedly".to_string(),
                        },
                        Err(e) => ResponseQuery {
                            code: Code::Err(3),
                            log: format!("Database error: {}", e),
                            info: "".to_string(),
                            index: 0,
                            key: vec![].into(),
                            value: vec![].into(),
                            proof_ops: None,
                            height: 0,
                            codespace: "sedly".to_string(),
                        }
                    }
                } else {
                    ResponseQuery {
                        code: Code::Err(4),
                        log: "Invalid height format".to_string(),
                        info: "".to_string(),
                        index: 0,
                        key: vec![].into(),
                        value: vec![].into(),
                        proof_ops: None,
                        height: 0,
                        codespace: "sedly".to_string(),
                    }
                }
            }
            ["info"] => {
                let chain_state = self.chain_state.lock().unwrap();
                let info = format!(
                    "{{\"height\":{},\"best_block\":\"{}\"}}",
                    chain_state.height,
                    hex::encode(chain_state.best_block_hash)
                );

                ResponseQuery {
                    code: Code::Ok,
                    log: "Chain info".to_string(),
                    info: "".to_string(),
                    index: 0,
                    key: vec![].into(),
                    value: info.into_bytes().into(),
                    proof_ops: None,
                    height: chain_state.height as i64,
                    codespace: "".to_string(),
                }
            }
            _ => ResponseQuery {
                code: Code::Err(5),
                log: "Unknown query path".to_string(),
                info: "".to_string(),
                index: 0,
                key: vec![].into(),
                value: vec![].into(),
                proof_ops: None,
                height: 0,
                codespace: "sedly".to_string(),
            }
        }
    }
}

/// Consensus errors
#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Block building error: {0}")]
    BlockBuildingError(String),

    #[error("Consensus error: {0}")]
    ConsensusError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_app() -> (SedlyApp, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let app = SedlyApp::new(temp_dir.path().to_str().unwrap()).unwrap();
        (app, temp_dir)
    }

    #[test]
    fn test_app_creation() {
        let (app, _temp) = create_test_app();
        let chain_state = app.chain_state.lock().unwrap();

        assert_eq!(chain_state.height, 0);
        assert_ne!(chain_state.best_block_hash, [0; 32]); // Should have genesis hash
    }

    #[test]
    fn test_info_request() {
        let (app, _temp) = create_test_app();

        let response = app.info(RequestInfo {
            version: "1.0".to_string(),
            block_version: 1,
            p2p_version: 1,
            abci_version: "1.0".to_string(),
        });

        assert_eq!(response.data, "Sedly Blockchain");
        assert_eq!(response.last_block_height, 0);
    }

    #[test]
    fn test_block_reward_calculation() {
        let (app, _temp) = create_test_app();

        // Initial reward
        assert_eq!(app.calculate_block_reward(0), INITIAL_BLOCK_REWARD);

        // After first halving
        assert_eq!(app.calculate_block_reward(HALVING_INTERVAL), INITIAL_BLOCK_REWARD / 2);

        // After second halving
        assert_eq!(app.calculate_block_reward(HALVING_INTERVAL * 2), INITIAL_BLOCK_REWARD / 4);
    }

    #[test]
    fn test_coinbase_creation() {
        let (app, _temp) = create_test_app();

        let coinbase = app.create_coinbase(0, b"test_address");

        assert!(coinbase.is_coinbase());
        assert_eq!(coinbase.outputs.len(), 1);
        assert_eq!(coinbase.outputs[0].value, INITIAL_BLOCK_REWARD);
    }
}