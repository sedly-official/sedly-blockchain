//! Block e BlockHeader structures per Sedly blockchain

use crate::transaction::Transaction;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Block header contenente metadati del block
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Versione del block format
    pub version: u32,
    /// Hash del block precedente
    pub previous_hash: [u8; 32],
    /// Merkle root delle transazioni
    pub merkle_root: [u8; 32],
    /// Timestamp Unix in secondi
    pub timestamp: u64,
    /// Difficulty target (formato compact come Bitcoin)
    pub bits: u32,
    /// Nonce per il mining
    pub nonce: u64,
    /// Altezza del block nella chain
    pub height: u64,
}

/// Block completo con header + transazioni
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Header del block
    pub header: BlockHeader,
    /// Lista delle transazioni nel block
    pub transactions: Vec<Transaction>,
}

impl BlockHeader {
    /// Crea nuovo block header
    pub fn new(
        version: u32,
        previous_hash: [u8; 32],
        merkle_root: [u8; 32],
        bits: u32,
        height: u64,
    ) -> Self {
        Self {
            version,
            previous_hash,
            merkle_root,
            timestamp: Self::current_timestamp(),
            bits,
            nonce: 0,
            height,
        }
    }

    /// Timestamp corrente in secondi Unix
    pub fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    /// Calcola hash del header (double SHA-256 come Bitcoin)
    pub fn hash(&self) -> [u8; 32] {
        let header_bytes = bincode::serialize(self)
            .expect("Failed to serialize header");

        // Double SHA-256
        let hash1 = Sha256::digest(&header_bytes);
        let hash2 = Sha256::digest(&hash1);

        hash2.into()
    }

    /// Converte bits in target hash per difficulty check
    pub fn target(&self) -> [u8; 32] {
        bits_to_target(self.bits)
    }

    /// Verifica se il hash soddisfa la difficulty
    pub fn meets_difficulty(&self) -> bool {
        let hash = self.hash();
        let target = self.target();
        hash <= target
    }
}

impl Block {
    /// Crea nuovo block
    pub fn new(
        previous_hash: [u8; 32],
        transactions: Vec<Transaction>,
        bits: u32,
        height: u64,
    ) -> Self {
        let merkle_root = Self::calculate_merkle_root(&transactions);
        let header = BlockHeader::new(
            crate::PROTOCOL_VERSION,
            previous_hash,
            merkle_root,
            bits,
            height,
        );

        Self {
            header,
            transactions,
        }
    }

    /// Hash del block (hash dell'header)
    pub fn hash(&self) -> [u8; 32] {
        self.header.hash()
    }

    /// Calcola merkle root delle transazioni
    pub fn calculate_merkle_root(transactions: &[Transaction]) -> [u8; 32] {
        if transactions.is_empty() {
            return [0; 32];
        }

        let mut hashes: Vec<[u8; 32]> = transactions
            .iter()
            .map(|tx| tx.hash())
            .collect();

        // Semplice merkle tree (TODO: implementazione completa)
        while hashes.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in hashes.chunks(2) {
                let combined_hash = if chunk.len() == 2 {
                    let mut combined = [0u8; 64];
                    combined[..32].copy_from_slice(&chunk[0]);
                    combined[32..].copy_from_slice(&chunk[1]);
                    combined
                } else {
                    // Se numero dispari, duplica l'ultimo hash
                    let mut combined = [0u8; 64];
                    combined[..32].copy_from_slice(&chunk[0]);
                    combined[32..].copy_from_slice(&chunk[0]);
                    combined
                };

                let hash = Sha256::digest(&combined_hash);
                next_level.push(hash.into());
            }

            hashes = next_level;
        }

        hashes[0]
    }

    /// Verifica che il block sia valido
    pub fn is_valid(&self) -> bool {
        // Verifica proof of work
        if !self.header.meets_difficulty() {
            return false;
        }

        // Verifica merkle root
        let calculated_root = Self::calculate_merkle_root(&self.transactions);
        if calculated_root != self.header.merkle_root {
            return false;
        }

        // Verifica che non sia vuoto (deve avere almeno coinbase)
        if self.transactions.is_empty() {
            return false;
        }

        // TODO: Verifica ogni transazione

        true
    }

    /// Dimensione del block in bytes
    pub fn size(&self) -> usize {
        bincode::serialize(self)
            .map(|bytes| bytes.len())
            .unwrap_or(0)
    }

    /// Crea genesis block
    pub fn genesis() -> Self {
        let genesis_tx = Transaction::genesis();

        Self {
            header: BlockHeader {
                version: crate::PROTOCOL_VERSION,
                previous_hash: [0; 32],
                merkle_root: genesis_tx.hash(),
                timestamp: 1704067200, // 1 Jan 2024 00:00:00 UTC
                bits: 0x1d00ffff, // Difficulty iniziale facile
                nonce: 0,
                height: 0,
            },
            transactions: vec![genesis_tx],
        }
    }
}

/// Converte compact bits in target hash (algoritmo Bitcoin)
pub fn bits_to_target(bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];

    let exponent = bits >> 24;
    let mantissa = bits & 0x00ffffff;

    if exponent <= 3 {
        let mantissa = mantissa >> (8 * (3 - exponent));
        target[28] = mantissa as u8;
        target[29] = (mantissa >> 8) as u8;
        target[30] = (mantissa >> 16) as u8;
    } else {
        let shift = exponent - 3;
        target[32 - shift as usize - 3] = mantissa as u8;
        target[32 - shift as usize - 2] = (mantissa >> 8) as u8;
        target[32 - shift as usize - 1] = (mantissa >> 16) as u8;
    }

    target
}

/// Converte target hash in compact bits
pub fn target_to_bits(target: &[u8; 32]) -> u32 {
    // Trova il primo byte non-zero
    let mut size = 32;
    while size > 0 && target[32 - size] == 0 {
        size -= 1;
    }

    if size == 0 {
        return 0;
    }

    let compact = if size <= 3 {
        (target[32 - size] as u32) |
            ((target[32 - size + 1] as u32) << 8) |
            ((target[32 - size + 2] as u32) << 16)
    } else {
        (target[32 - size] as u32) |
            ((target[32 - size + 1] as u32) << 8) |
            ((target[32 - size + 2] as u32) << 16)
    };

    compact | ((size as u32) << 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_header_hash() {
        let header = BlockHeader::new(
            1,
            [0; 32],
            [0; 32],
            0x1d00ffff,
            0,
        );

        let hash = header.hash();
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, [0; 32]);
    }

    #[test]
    fn test_genesis_block() {
        let genesis = Block::genesis();
        assert_eq!(genesis.header.height, 0);
        assert_eq!(genesis.header.previous_hash, [0; 32]);
        assert_eq!(genesis.transactions.len(), 1);
    }

    #[test]
    fn test_bits_conversion() {
        let bits = 0x1d00ffff;
        let target = bits_to_target(bits);
        let converted_back = target_to_bits(&target);
        assert_eq!(bits, converted_back);
    }
}