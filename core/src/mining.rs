//! Mining SHA-256 implementation per Sedly blockchain

use crate::{Block, BlockHeader, Transaction};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Miner per il mining di nuovi blocks
pub struct Miner {
    /// Target difficulty corrente
    pub target: [u8; 32],
    /// Numero di thread per mining
    pub threads: usize,
    /// Flag per stop del mining
    pub should_stop: Arc<AtomicBool>,
    /// Nonce counter globale per evitare duplicati
    pub nonce_counter: Arc<AtomicU64>,
}

/// Risultato del mining
#[derive(Debug, Clone)]
pub struct MiningResult {
    /// Block minato con successo
    pub block: Block,
    /// Numero di hash calcolati
    pub hashes_calculated: u64,
    /// Tempo impiegato per mining
    pub mining_time: Duration,
    /// Hash rate medio (hashes per secondo)
    pub hash_rate: f64,
}

/// Statistiche mining per progress reporting
#[derive(Debug, Clone)]
pub struct MiningStats {
    /// Hash calcolati fino ad ora
    pub total_hashes: u64,
    /// Tempo di mining trascorso
    pub elapsed_time: Duration,
    /// Hash rate corrente (H/s)
    pub current_hash_rate: f64,
    /// Target difficulty
    pub target: [u8; 32],
    /// Nonce corrente
    pub current_nonce: u64,
}

impl Miner {
    /// Crea nuovo miner
    pub fn new(target: [u8; 32], threads: usize) -> Self {
        Self {
            target,
            threads,
            should_stop: Arc::new(AtomicBool::new(false)),
            nonce_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Crea miner con difficulty bits
    pub fn with_difficulty_bits(bits: u32, threads: usize) -> Self {
        let target = crate::block::bits_to_target(bits);
        Self::new(target, threads)
    }

    /// Avvia mining di un nuovo block
    pub fn mine_block(
        &self,
        previous_hash: [u8; 32],
        transactions: Vec<Transaction>,
        height: u64,
        bits: u32,
    ) -> Result<MiningResult, MiningError> {
        let start_time = Instant::now();
        self.should_stop.store(false, Ordering::Relaxed);
        self.nonce_counter.store(0, Ordering::Relaxed);

        // Crea template del block
        let merkle_root = Block::calculate_merkle_root(&transactions);
        let mut header = BlockHeader {
            version: crate::PROTOCOL_VERSION,
            previous_hash,
            merkle_root,
            timestamp: Self::current_timestamp(),
            bits,
            nonce: 0,
            height,
        };

        // Mining loop principale
        let mut total_hashes = 0u64;
        let mut last_stats_time = start_time;
        let stats_interval = Duration::from_secs(5);

        loop {
            // Check stop flag
            if self.should_stop.load(Ordering::Relaxed) {
                return Err(MiningError::Stopped);
            }

            // Prova mining per batch di nonce
            let batch_size = 100_000u64;
            let start_nonce = self.nonce_counter.fetch_add(batch_size, Ordering::Relaxed);

            for nonce_offset in 0..batch_size {
                header.nonce = start_nonce + nonce_offset;
                total_hashes += 1;

                if self.check_proof_of_work(&header) {
                    // Mining successful!
                    let mining_time = start_time.elapsed();
                    let hash_rate = total_hashes as f64 / mining_time.as_secs_f64();

                    let block = Block {
                        header,
                        transactions,
                    };

                    return Ok(MiningResult {
                        block,
                        hashes_calculated: total_hashes,
                        mining_time,
                        hash_rate,
                    });
                }

                // Update timestamp periodically (every 1M hashes)
                if total_hashes % 1_000_000 == 0 {
                    header.timestamp = Self::current_timestamp();
                }
            }

            // Print stats periodically
            let now = Instant::now();
            if now.duration_since(last_stats_time) >= stats_interval {
                let elapsed = now.duration_since(start_time);
                let hash_rate = total_hashes as f64 / elapsed.as_secs_f64();

                log::info!(
                    "Mining stats: {} hashes, {:.2} H/s, nonce: {}, elapsed: {:?}",
                    total_hashes,
                    hash_rate,
                    header.nonce,
                    elapsed
                );

                last_stats_time = now;
            }
        }
    }

    /// Controlla se l'header soddisfa la proof of work
    fn check_proof_of_work(&self, header: &BlockHeader) -> bool {
        let hash = header.hash();
        hash <= self.target
    }

    /// Timestamp Unix corrente
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    /// Stop del mining
    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }

    /// Ottiene statistiche mining correnti
    pub fn get_stats(&self, start_time: Instant) -> MiningStats {
        let elapsed = start_time.elapsed();
        let total_hashes = self.nonce_counter.load(Ordering::Relaxed);
        let hash_rate = if elapsed.as_secs() > 0 {
            total_hashes as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        MiningStats {
            total_hashes,
            elapsed_time: elapsed,
            current_hash_rate: hash_rate,
            target: self.target,
            current_nonce: total_hashes,
        }
    }

    /// Mining multi-threaded (avanzato)
    pub fn mine_block_threaded(
        &self,
        previous_hash: [u8; 32],
        transactions: Vec<Transaction>,
        height: u64,
        bits: u32,
    ) -> Result<MiningResult, MiningError> {
        use std::thread;

        let start_time = Instant::now();
        self.should_stop.store(false, Ordering::Relaxed);
        self.nonce_counter.store(0, Ordering::Relaxed);

        // Shared template
        let merkle_root = Block::calculate_merkle_root(&transactions);
        let header_template = BlockHeader {
            version: crate::PROTOCOL_VERSION,
            previous_hash,
            merkle_root,
            timestamp: Self::current_timestamp(),
            bits,
            nonce: 0,
            height,
        };

        // Result channel
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn mining threads
        let mut handles = Vec::new();
        for thread_id in 0..self.threads {
            let tx = tx.clone();
            let template = header_template.clone();
            let target = self.target;
            let should_stop = Arc::clone(&self.should_stop);
            let nonce_counter = Arc::clone(&self.nonce_counter);
            let transactions = transactions.clone();

            let handle = thread::spawn(move || {
                let mut header = template;
                let mut local_hashes = 0u64;

                loop {
                    if should_stop.load(Ordering::Relaxed) {
                        break;
                    }

                    // Get nonce range for this thread
                    let start_nonce = nonce_counter.fetch_add(10000, Ordering::Relaxed);

                    for nonce_offset in 0..10000 {
                        header.nonce = start_nonce + nonce_offset;
                        local_hashes += 1;

                        let hash = header.hash();
                        if hash <= target {
                            // Found solution!
                            let block = Block {
                                header,
                                transactions: transactions.clone(),
                            };

                            let result = MiningResult {
                                block,
                                hashes_calculated: local_hashes,
                                mining_time: start_time.elapsed(),
                                hash_rate: local_hashes as f64 / start_time.elapsed().as_secs_f64(),
                            };

                            let _ = tx.send(Ok(result));
                            return;
                        }
                    }

                    // Update timestamp occasionally
                    if local_hashes % 100_000 == 0 {
                        header.timestamp = Self::current_timestamp();
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for result or timeout
        let result = match rx.recv_timeout(Duration::from_secs(300)) {
            Ok(result) => {
                self.should_stop.store(true, Ordering::Relaxed);
                result
            }
            Err(_) => {
                self.should_stop.store(true, Ordering::Relaxed);
                Err(MiningError::Timeout)
            }
        };

        // Wait for all threads to finish
        for handle in handles {
            let _ = handle.join();
        }

        result
    }

    /// Verifica se un block hash soddisfa il target
    pub fn verify_block_hash(block: &Block, target: &[u8; 32]) -> bool {
        let hash = block.hash();
        hash <= *target
    }

    /// Calcola hash rate teorico per difficulty
    pub fn calculate_expected_time(target: &[u8; 32], hash_rate: f64) -> Duration {
        // Calcola il numero di tentativi necessari
        let max_target = [0xff; 32];
        let target_num = u256_from_bytes(target);
        let max_num = u256_from_bytes(&max_target);

        let attempts = (max_num as f64) / (target_num as f64);
        let seconds = attempts / hash_rate;

        Duration::from_secs_f64(seconds)
    }
}

/// Converte array di 32 bytes in approssimazione u64 per calcoli
fn u256_from_bytes(bytes: &[u8; 32]) -> u64 {
    // Prende solo gli ultimi 8 bytes per approssimazione
    u64::from_be_bytes([
        bytes[24], bytes[25], bytes[26], bytes[27],
        bytes[28], bytes[29], bytes[30], bytes[31]
    ])
}

/// Errori del mining
#[derive(Debug, Clone, thiserror::Error)]
pub enum MiningError {
    #[error("Mining stopped by user")]
    Stopped,
    #[error("Mining timeout")]
    Timeout,
    #[error("Invalid block template: {0}")]
    InvalidTemplate(String),
}

/// Utility functions
impl MiningStats {
    /// Formatta hash rate in modo leggibile
    pub fn format_hash_rate(&self) -> String {
        if self.current_hash_rate >= 1_000_000_000.0 {
            format!("{:.2} GH/s", self.current_hash_rate / 1_000_000_000.0)
        } else if self.current_hash_rate >= 1_000_000.0 {
            format!("{:.2} MH/s", self.current_hash_rate / 1_000_000.0)
        } else if self.current_hash_rate >= 1_000.0 {
            format!("{:.2} KH/s", self.current_hash_rate / 1_000.0)
        } else {
            format!("{:.2} H/s", self.current_hash_rate)
        }
    }

    /// Stima tempo rimanente per trovare block
    pub fn estimated_time_to_block(&self) -> Option<Duration> {
        if self.current_hash_rate <= 0.0 {
            return None;
        }

        let expected_time = Miner::calculate_expected_time(&self.target, self.current_hash_rate);
        Some(expected_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Transaction;

    #[test]
    fn test_miner_creation() {
        let target = [0x0f; 32]; // Easy target
        let miner = Miner::new(target, 1);

        assert_eq!(miner.target, target);
        assert_eq!(miner.threads, 1);
        assert!(!miner.should_stop.load(Ordering::Relaxed));
    }

    #[test]
    fn test_mining_easy_target() {
        let mut target = [0xff; 32];
        target[0] = 0x0f; // Very easy target

        let miner = Miner::new(target, 1);
        let transactions = vec![Transaction::genesis()];

        let result = miner.mine_block([0; 32], transactions, 1, 0x1d00ffff);
        assert!(result.is_ok());

        let mining_result = result.unwrap();
        assert!(mining_result.hashes_calculated > 0);
        assert!(mining_result.hash_rate > 0.0);
    }

    #[test]
    fn test_proof_of_work_verification() {
        let target = [0x0f; 32];
        let miner = Miner::new(target, 1);

        let mut header = BlockHeader {
            version: 1,
            previous_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 1704067200,
            bits: 0x1d00ffff,
            nonce: 0,
            height: 1,
        };

        // Test with easy target - should find solution quickly
        let mut found = false;
        for nonce in 0..10000 {
            header.nonce = nonce;
            if miner.check_proof_of_work(&header) {
                found = true;
                break;
            }
        }

        assert!(found, "Should find valid nonce with easy target");
    }

    #[test]
    fn test_hash_rate_formatting() {
        let stats = MiningStats {
            total_hashes: 1000,
            elapsed_time: Duration::from_secs(1),
            current_hash_rate: 1_500_000.0,
            target: [0; 32],
            current_nonce: 1000,
        };

        assert_eq!(stats.format_hash_rate(), "1.50 MH/s");
    }

    #[test]
    fn test_target_to_bits_conversion() {
        let bits = 0x1d00ffff;
        let target = crate::block::bits_to_target(bits);
        let miner = Miner::with_difficulty_bits(bits, 1);

        assert_eq!(miner.target, target);
    }
}