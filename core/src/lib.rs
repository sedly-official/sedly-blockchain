//! Sedly Core - Strutture dati fondamentali della blockchain

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

// Re-export dei moduli principali
pub mod block;
pub mod transaction;
pub mod mining;
pub mod difficulty;
pub mod validation;
pub mod storage;  // <- Aggiungi questa riga

// Re-export dei tipi principali
pub use block::{Block, BlockHeader};
pub use transaction::{Transaction, TxInput, TxOutput, OutPoint};
pub use storage::{BlockchainDB, ChainMetadata, UtxoEntry, DatabaseStats, StorageError};  // <- Aggiungi questa riga

/// Versione attuale del protocollo
pub const PROTOCOL_VERSION: u32 = 1;

/// Reward per block in satoshi (50 SLY iniziali, come Bitcoin)
pub const INITIAL_BLOCK_REWARD: u64 = 50_00000000; // 50.00000000 SLY

/// Target time per block in secondi (2 minuti, più veloce di Bitcoin)
pub const TARGET_BLOCK_TIME: u64 = 120;

/// Blocks per difficulty adjustment (144 blocks = ~4.8 ore)
pub const DIFFICULTY_ADJUSTMENT_INTERVAL: u64 = 144;

/// Massimo adjustment della difficulty per periodo (4x come Bitcoin)
pub const MAX_DIFFICULTY_ADJUSTMENT: f64 = 4.0;

/// Genesis block hash (sarà calcolato al primo avvio)
pub const GENESIS_HASH: [u8; 32] = [0; 32];

/// Halving interval (ogni 210,000 blocks come Bitcoin)
pub const HALVING_INTERVAL: u64 = 210_000;

/// Dimensione massima block in bytes (1MB iniziale, espandibile)
pub const MAX_BLOCK_SIZE: usize = 1_000_000;

/// Fee minima per transazione (1000 satoshi)
pub const MIN_TX_FEE: u64 = 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(TARGET_BLOCK_TIME, 120); // 2 minuti
        assert_eq!(DIFFICULTY_ADJUSTMENT_INTERVAL, 144);
    }

    #[test]
    fn test_reward_calculation() {
        assert_eq!(INITIAL_BLOCK_REWARD, 5_000_000_000); // 50 SLY in satoshi

        // Test halving
        assert_eq!(INITIAL_BLOCK_REWARD / 2, 2_500_000_000); // 25 SLY dopo halving
    }
}