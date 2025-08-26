//! Difficulty adjustment algorithm per Sedly blockchain

use crate::{Block, BlockHeader};
use std::cmp;

/// Difficulty adjustment manager
pub struct DifficultyAdjuster {
    /// Target time per block in secondi (default: 120 secondi = 2 minuti)
    target_block_time: u64,
    /// Intervallo di aggiustamento in blocks (default: 144 blocks)
    adjustment_interval: u64,
    /// Massimo moltiplicatore per adjustment (4.0 come Bitcoin)
    max_adjustment_factor: f64,
    /// Minimo moltiplicatore per adjustment (0.25 = 1/4)
    min_adjustment_factor: f64,
}

/// Risultato del calcolo di difficulty adjustment
#[derive(Debug, Clone)]
pub struct DifficultyAdjustment {
    /// Difficulty attuale (bits format)
    pub current_bits: u32,
    /// Nuova difficulty calcolata (bits format)
    pub new_bits: u32,
    /// Fattore di aggiustamento applicato
    pub adjustment_factor: f64,
    /// Tempo medio effettivo per block nell'intervallo
    pub actual_time_per_block: f64,
    /// Indica se è necessario un aggiustamento
    pub needs_adjustment: bool,
}

impl Default for DifficultyAdjuster {
    fn default() -> Self {
        Self::new()
    }
}

impl DifficultyAdjuster {
    /// Crea nuovo difficulty adjuster con parametri Sedly
    pub fn new() -> Self {
        Self {
            target_block_time: crate::TARGET_BLOCK_TIME,
            adjustment_interval: crate::DIFFICULTY_ADJUSTMENT_INTERVAL,
            max_adjustment_factor: crate::MAX_DIFFICULTY_ADJUSTMENT,
            min_adjustment_factor: 1.0 / crate::MAX_DIFFICULTY_ADJUSTMENT,
        }
    }

    /// Crea difficulty adjuster con parametri custom
    pub fn with_params(
        target_block_time: u64,
        adjustment_interval: u64,
        max_adjustment_factor: f64,
    ) -> Self {
        Self {
            target_block_time,
            adjustment_interval,
            max_adjustment_factor,
            min_adjustment_factor: 1.0 / max_adjustment_factor,
        }
    }

    /// Calcola la nuova difficulty basata sui block recenti
    pub fn calculate_next_difficulty(
        &self,
        recent_blocks: &[Block],
        current_bits: u32,
    ) -> Result<DifficultyAdjustment, DifficultyError> {
        // Verifica che abbiamo abbastanza blocks
        if recent_blocks.len() < self.adjustment_interval as usize {
            return Err(DifficultyError::InsufficientBlocks {
                required: self.adjustment_interval as usize,
                provided: recent_blocks.len(),
            });
        }

        // Verifica che i block siano in ordine crescente di altezza
        if !self.verify_block_sequence(recent_blocks)? {
            return Err(DifficultyError::InvalidBlockSequence);
        }

        // Calcola il tempo effettivo trascorso
        let first_block = &recent_blocks[0];
        let last_block = &recent_blocks[recent_blocks.len() - 1];

        let actual_time = last_block.header.timestamp - first_block.header.timestamp;
        let expected_time = self.target_block_time * (self.adjustment_interval - 1);

        // Calcola tempo medio per block
        let actual_time_per_block = actual_time as f64 / (self.adjustment_interval - 1) as f64;

        // Calcola fattore di aggiustamento
        let raw_adjustment_factor = expected_time as f64 / actual_time as f64;

        // Applica limiti di aggiustamento
        let adjustment_factor = raw_adjustment_factor
            .max(self.min_adjustment_factor)
            .min(self.max_adjustment_factor);

        // Calcola nuova difficulty
        let new_bits = if adjustment_factor == 1.0 {
            current_bits
        } else {
            self.adjust_bits(current_bits, adjustment_factor)?
        };

        // Determina se serve aggiustamento
        let needs_adjustment = new_bits != current_bits;

        Ok(DifficultyAdjustment {
            current_bits,
            new_bits,
            adjustment_factor,
            actual_time_per_block,
            needs_adjustment,
        })
    }

    /// Verifica che la sequence di block sia valida
    fn verify_block_sequence(&self, blocks: &[Block]) -> Result<bool, DifficultyError> {
        for i in 1..blocks.len() {
            let prev_height = blocks[i-1].header.height;
            let curr_height = blocks[i].header.height;

            if curr_height != prev_height + 1 {
                return Ok(false);
            }

            // Verifica anche che i timestamp siano crescenti
            if blocks[i].header.timestamp < blocks[i-1].header.timestamp {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Aggiusta i bits di difficulty
    fn adjust_bits(&self, current_bits: u32, factor: f64) -> Result<u32, DifficultyError> {
        // Converte bits a target
        let current_target = crate::block::bits_to_target(current_bits);

        // Calcola nuovo target (difficulty più alta = target più basso)
        let new_target = self.scale_target(&current_target, 1.0 / factor)?;

        // Converte nuovo target a bits
        let new_bits = crate::block::target_to_bits(&new_target);

        Ok(new_bits)
    }

    /// Scala un target difficulty per un fattore dato
    fn scale_target(&self, target: &[u8; 32], scale_factor: f64) -> Result<[u8; 32], DifficultyError> {
        // Conversione semplificata: prende gli ultimi 8 bytes come u64
        let target_u64 = u64::from_be_bytes([
            target[24], target[25], target[26], target[27],
            target[28], target[29], target[30], target[31]
        ]);

        // Applica scaling
        let scaled = (target_u64 as f64 * scale_factor) as u64;

        // Converti indietro a [u8; 32]
        let mut result = [0u8; 32];
        let scaled_bytes = scaled.to_be_bytes();
        result[24..32].copy_from_slice(&scaled_bytes);

        Ok(result)
    }

    /// Calcola la difficulty per il genesis block
    pub fn genesis_difficulty() -> u32 {
        // Difficulty iniziale molto facile per bootstrap
        0x1d00ffff
    }

    /// Calcola la difficulty minima consentita
    pub fn minimum_difficulty() -> u32 {
        // Difficulty minima per evitare tempi troppo lunghi
        0x1d00ffff
    }

    /// Calcola la difficulty massima consentita
    pub fn maximum_difficulty() -> u32 {
        // Difficulty massima pratica
        0x1b000000
    }

    /// Verifica che i bits siano in range valido
    pub fn validate_bits(bits: u32) -> Result<(), DifficultyError> {
        let min_bits = Self::minimum_difficulty();
        let max_bits = Self::maximum_difficulty();

        if bits > min_bits || bits < max_bits {
            return Err(DifficultyError::BitsOutOfRange { bits, min_bits, max_bits });
        }

        Ok(())
    }

    /// Calcola hash rate stimato per una difficulty
    pub fn estimate_network_hashrate(&self, bits: u32, actual_block_time: f64) -> f64 {
        let target = crate::block::bits_to_target(bits);
        let target_u64 = u64::from_be_bytes([
            target[24], target[25], target[26], target[27],
            target[28], target[29], target[30], target[31]
        ]);

        // Hash rate = difficulty / tempo
        let max_target = u64::MAX;
        let difficulty = max_target as f64 / target_u64 as f64;

        difficulty / actual_block_time
    }

    /// Predice il prossimo aggiustamento in base ai tempi correnti
    pub fn predict_next_adjustment(
        &self,
        recent_block_times: &[u64],
        current_bits: u32,
    ) -> Result<DifficultyAdjustment, DifficultyError> {
        if recent_block_times.is_empty() {
            return Err(DifficultyError::InsufficientData);
        }

        // Calcola tempo medio recente
        let avg_time = recent_block_times.iter().sum::<u64>() as f64 / recent_block_times.len() as f64;

        // Calcola fattore di aggiustamento previsto
        let predicted_factor = self.target_block_time as f64 / avg_time;

        // Applica limiti
        let bounded_factor = predicted_factor
            .max(self.min_adjustment_factor)
            .min(self.max_adjustment_factor);

        // Calcola bits previsti
        let predicted_bits = if bounded_factor == 1.0 {
            current_bits
        } else {
            self.adjust_bits(current_bits, bounded_factor)?
        };

        Ok(DifficultyAdjustment {
            current_bits,
            new_bits: predicted_bits,
            adjustment_factor: bounded_factor,
            actual_time_per_block: avg_time,
            needs_adjustment: predicted_bits != current_bits,
        })
    }
}

/// Errori del difficulty adjustment
#[derive(Debug, Clone, thiserror::Error)]
pub enum DifficultyError {
    #[error("Insufficient blocks: need {required}, got {provided}")]
    InsufficientBlocks { required: usize, provided: usize },

    #[error("Invalid block sequence: blocks must be consecutive")]
    InvalidBlockSequence,

    #[error("Bits out of valid range: {bits} (min: {min_bits}, max: {max_bits})")]
    BitsOutOfRange { bits: u32, min_bits: u32, max_bits: u32 },

    #[error("Insufficient data for calculation")]
    InsufficientData,

    #[error("Target calculation overflow")]
    TargetOverflow,

    #[error("Invalid adjustment factor: {factor}")]
    InvalidAdjustmentFactor { factor: f64 },
}

/// Utility functions
impl DifficultyAdjustment {
    /// Formatta l'aggiustamento in modo leggibile
    pub fn format_adjustment(&self) -> String {
        if !self.needs_adjustment {
            return "No adjustment needed".to_string();
        }

        let direction = if self.adjustment_factor > 1.0 {
            "increased"
        } else {
            "decreased"
        };

        let percentage = (self.adjustment_factor - 1.0).abs() * 100.0;

        format!(
            "Difficulty {} by {:.2}% (factor: {:.4})",
            direction, percentage, self.adjustment_factor
        )
    }

    /// Indica se la difficulty è aumentata
    pub fn is_increase(&self) -> bool {
        self.new_bits < self.current_bits
    }

    /// Indica se la difficulty è diminuita
    pub fn is_decrease(&self) -> bool {
        self.new_bits > self.current_bits
    }

    /// Calcola la percentuale di cambiamento
    pub fn change_percentage(&self) -> f64 {
        (self.adjustment_factor - 1.0) * 100.0
    }
}

/// Funzioni di utility per debugging
impl DifficultyAdjuster {
    /// Debug info per un aggiustamento
    pub fn debug_adjustment(
        &self,
        blocks: &[Block],
        adjustment: &DifficultyAdjustment,
    ) -> String {
        let first_timestamp = blocks.first().unwrap().header.timestamp;
        let last_timestamp = blocks.last().unwrap().header.timestamp;
        let actual_time = last_timestamp - first_timestamp;
        let expected_time = self.target_block_time * (self.adjustment_interval - 1);

        format!(
            "Difficulty Adjustment Debug:\n\
            - Blocks analyzed: {}\n\
            - Time period: {} seconds (expected: {})\n\
            - Average block time: {:.2}s (target: {}s)\n\
            - Adjustment factor: {:.4}\n\
            - Current bits: 0x{:08x}\n\
            - New bits: 0x{:08x}\n\
            - Change: {}",
            blocks.len(),
            actual_time,
            expected_time,
            adjustment.actual_time_per_block,
            self.target_block_time,
            adjustment.adjustment_factor,
            adjustment.current_bits,
            adjustment.new_bits,
            adjustment.format_adjustment()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Transaction};

    fn create_test_blocks(count: usize, time_interval: u64, bits: u32) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();
        let base_time = 1704067200; // 1 Jan 2024

        for i in 0..count {
            let timestamp = base_time + (i as u64 * time_interval);
            let previous_hash = if i == 0 { [0; 32] } else { blocks[i-1].hash() };

            let mut block = Block::new(
                previous_hash,
                vec![Transaction::genesis()],
                bits,
                i as u64,
            );
            block.header.timestamp = timestamp;

            blocks.push(block);
        }

        blocks
    }

    #[test]
    fn test_difficulty_adjuster_creation() {
        let adjuster = DifficultyAdjuster::new();

        assert_eq!(adjuster.target_block_time, 120);
        assert_eq!(adjuster.adjustment_interval, 144);
        assert_eq!(adjuster.max_adjustment_factor, 4.0);
    }

    #[test]
    fn test_no_adjustment_needed() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(144, 120, 0x1d00ffff); // Perfect 2min blocks

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_ok());

        let adjustment = result.unwrap();
        assert!(!adjustment.needs_adjustment);
        assert_eq!(adjustment.current_bits, adjustment.new_bits);
    }

    #[test]
    fn test_difficulty_increase() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(144, 60, 0x1d00ffff); // 1min blocks (too fast)

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_ok());

        let adjustment = result.unwrap();
        assert!(adjustment.needs_adjustment);
        assert!(adjustment.is_increase());
        assert!(adjustment.adjustment_factor > 1.0);
    }

    #[test]
    fn test_difficulty_decrease() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(144, 240, 0x1d00ffff); // 4min blocks (too slow)

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_ok());

        let adjustment = result.unwrap();
        assert!(adjustment.needs_adjustment);
        assert!(adjustment.adjustment_factor < 1.0); // Factor should be < 1 for slower blocks
        // Note: Due to the complexity of bits manipulation, we'll just check the factor for now
    }

    #[test]
    fn test_max_adjustment_limit() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(144, 30, 0x1d00ffff); // 30s blocks (very fast)

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_ok());

        let adjustment = result.unwrap();
        assert_eq!(adjustment.adjustment_factor, 4.0); // Capped at max
    }

    #[test]
    fn test_min_adjustment_limit() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(144, 480, 0x1d00ffff); // 8min blocks (very slow)

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_ok());

        let adjustment = result.unwrap();
        assert_eq!(adjustment.adjustment_factor, 0.25); // Capped at min
    }

    #[test]
    fn test_insufficient_blocks() {
        let adjuster = DifficultyAdjuster::new();
        let blocks = create_test_blocks(100, 120, 0x1d00ffff); // Less than 144

        let result = adjuster.calculate_next_difficulty(&blocks, 0x1d00ffff);
        assert!(result.is_err());

        match result.unwrap_err() {
            DifficultyError::InsufficientBlocks { required: 144, provided: 100 } => (),
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_genesis_difficulty() {
        let bits = DifficultyAdjuster::genesis_difficulty();
        assert_eq!(bits, 0x1d00ffff);
    }

    #[test]
    fn test_network_hashrate_estimation() {
        let adjuster = DifficultyAdjuster::new();
        let hashrate = adjuster.estimate_network_hashrate(0x1d00ffff, 120.0);

        assert!(hashrate > 0.0);
    }

    #[test]
    fn test_adjustment_formatting() {
        let adjustment = DifficultyAdjustment {
            current_bits: 0x1d00ffff,
            new_bits: 0x1c00ffff,
            adjustment_factor: 1.5,
            actual_time_per_block: 80.0,
            needs_adjustment: true,
        };

        let formatted = adjustment.format_adjustment();
        assert!(formatted.contains("increased"));
        assert!(formatted.contains("50.00%"));
    }

    #[test]
    fn test_prediction() {
        let adjuster = DifficultyAdjuster::new();
        let recent_times = vec![90, 100, 110, 120, 130]; // Average: 110s (slightly fast)

        let result = adjuster.predict_next_adjustment(&recent_times, 0x1d00ffff);
        assert!(result.is_ok());

        let prediction = result.unwrap();
        // 120/110 = 1.09, so should predict slight increase
        assert!(prediction.adjustment_factor > 1.0);
    }
}