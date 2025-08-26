//! Consensus state management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Consensus state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusState {
    /// Current blockchain height
    pub height: u64,
    /// Best block hash
    pub best_block_hash: [u8; 32],
    /// Current difficulty bits
    pub difficulty_bits: u32,
    /// Total number of transactions
    pub total_transactions: u64,
    /// Validator set (for future PoS transition)
    pub validators: HashMap<String, ValidatorInfo>,
    /// Application state hash
    pub app_hash: [u8; 32],
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    /// Validator public key
    pub public_key: Vec<u8>,
    /// Voting power
    pub power: i64,
    /// Whether validator is active
    pub active: bool,
}

/// State manager for consensus
pub struct StateManager {
    /// Current state
    state: Arc<RwLock<ConsensusState>>,
    /// State history for rollback
    history: Arc<RwLock<Vec<ConsensusState>>>,
    /// Maximum history to keep
    max_history: usize,
}

impl StateManager {
    /// Create new state manager with initial state
    pub fn new(initial_state: ConsensusState) -> Self {
        Self {
            state: Arc::new(RwLock::new(initial_state)),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 100, // Keep last 100 states
        }
    }

    /// Get current state snapshot
    pub fn get_state(&self) -> ConsensusState {
        self.state.read().unwrap().clone()
    }

    /// Update state and save to history
    pub fn update_state<F>(&self, updater: F) -> Result<(), StateError>
    where
        F: FnOnce(&mut ConsensusState) -> Result<(), StateError>,
    {
        // Save current state to history
        {
            let current_state = self.state.read().unwrap().clone();
            let mut history = self.history.write().unwrap();
            history.push(current_state);

            // Trim history if too long
            if history.len() > self.max_history {
                history.remove(0);
            }
        }

        // Update state
        {
            let mut state = self.state.write().unwrap();
            updater(&mut *state)?;
        }

        Ok(())
    }

    /// Rollback to previous state
    pub fn rollback(&self) -> Result<(), StateError> {
        let mut history = self.history.write().unwrap();

        if let Some(previous_state) = history.pop() {
            let mut current_state = self.state.write().unwrap();
            *current_state = previous_state;
            Ok(())
        } else {
            Err(StateError::NoHistoryAvailable)
        }
    }

    /// Get state at specific height from history
    pub fn get_state_at_height(&self, height: u64) -> Option<ConsensusState> {
        let history = self.history.read().unwrap();
        history.iter()
            .find(|state| state.height == height)
            .cloned()
    }

    /// Advance to next block
    pub fn advance_block(
        &self,
        new_block_hash: [u8; 32],
        transactions_count: u64,
        new_difficulty: Option<u32>
    ) -> Result<(), StateError> {
        self.update_state(|state| {
            state.height += 1;
            state.best_block_hash = new_block_hash;
            state.total_transactions += transactions_count;

            if let Some(difficulty) = new_difficulty {
                state.difficulty_bits = difficulty;
            }

            // Update app hash (simple combination of block hash + height)
            let mut hasher = sha2::Sha256::new();
            hasher.update(&new_block_hash);
            hasher.update(&state.height.to_be_bytes());
            let hash_result = hasher.finalize();
            state.app_hash.copy_from_slice(&hash_result[..32]);

            Ok(())
        })
    }

    /// Add or update validator
    pub fn update_validator(
        &self,
        validator_id: String,
        public_key: Vec<u8>,
        power: i64,
        active: bool,
    ) -> Result<(), StateError> {
        self.update_state(|state| {
            let validator_info = ValidatorInfo {
                public_key,
                power,
                active,
            };

            state.validators.insert(validator_id, validator_info);
            Ok(())
        })
    }

    /// Remove validator
    pub fn remove_validator(&self, validator_id: &str) -> Result<(), StateError> {
        self.update_state(|state| {
            state.validators.remove(validator_id);
            Ok(())
        })
    }

    /// Get active validators
    pub fn get_active_validators(&self) -> Vec<(String, ValidatorInfo)> {
        let state = self.state.read().unwrap();
        state.validators
            .iter()
            .filter(|(_, info)| info.active)
            .map(|(id, info)| (id.clone(), info.clone()))
            .collect()
    }

    /// Validate state consistency
    pub fn validate_state(&self) -> Result<(), StateError> {
        let state = self.state.read().unwrap();

        // Basic validations
        if state.best_block_hash == [0; 32] && state.height > 0 {
            return Err(StateError::InvalidState(
                "Non-genesis block cannot have zero hash".to_string()
            ));
        }

        if state.difficulty_bits == 0 {
            return Err(StateError::InvalidState(
                "Difficulty bits cannot be zero".to_string()
            ));
        }

        // Validate validators
        for (id, validator) in &state.validators {
            if id.is_empty() {
                return Err(StateError::InvalidState(
                    "Validator ID cannot be empty".to_string()
                ));
            }

            if validator.public_key.is_empty() {
                return Err(StateError::InvalidState(
                    format!("Validator {} has empty public key", id)
                ));
            }
        }

        Ok(())
    }

    /// Export state for backup/migration
    pub fn export_state(&self) -> Result<Vec<u8>, StateError> {
        let state = self.state.read().unwrap();
        bincode::serialize(&*state)
            .map_err(|e| StateError::SerializationError(e.to_string()))
    }

    /// Import state from backup
    pub fn import_state(&self, data: &[u8]) -> Result<(), StateError> {
        let new_state: ConsensusState = bincode::deserialize(data)
            .map_err(|e| StateError::SerializationError(e.to_string()))?;

        // Validate imported state
        let temp_manager = StateManager::new(new_state.clone());
        temp_manager.validate_state()?;

        // Update current state
        let mut state = self.state.write().unwrap();
        *state = new_state;

        Ok(())
    }

    /// Get state statistics
    pub fn get_statistics(&self) -> StateStatistics {
        let state = self.state.read().unwrap();
        let history = self.history.read().unwrap();

        StateStatistics {
            current_height: state.height,
            total_transactions: state.total_transactions,
            active_validators: state.validators.iter()
                .filter(|(_, info)| info.active)
                .count() as u64,
            total_validators: state.validators.len() as u64,
            history_depth: history.len() as u64,
            current_difficulty: state.difficulty_bits,
        }
    }
}

/// State statistics
#[derive(Debug, Clone)]
pub struct StateStatistics {
    /// Current blockchain height
    pub current_height: u64,
    /// Total transactions processed
    pub total_transactions: u64,
    /// Number of active validators
    pub active_validators: u64,
    /// Total number of validators
    pub total_validators: u64,
    /// Depth of state history
    pub history_depth: u64,
    /// Current difficulty
    pub current_difficulty: u32,
}

impl Default for ConsensusState {
    fn default() -> Self {
        Self {
            height: 0,
            best_block_hash: [0; 32],
            difficulty_bits: 0x1d00ffff, // Initial difficulty
            total_transactions: 0,
            validators: HashMap::new(),
            app_hash: [0; 32],
        }
    }
}

/// State management errors
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("No history available for rollback")]
    NoHistoryAvailable,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Validator error: {0}")]
    ValidatorError(String),

    #[error("State update failed: {0}")]
    UpdateFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_manager_creation() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        let state = manager.get_state();
        assert_eq!(state.height, 0);
        assert_eq!(state.best_block_hash, [0; 32]);
    }

    #[test]
    fn test_state_update() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        // Update state
        manager.update_state(|state| {
            state.height = 100;
            state.total_transactions = 50;
            Ok(())
        }).unwrap();

        let state = manager.get_state();
        assert_eq!(state.height, 100);
        assert_eq!(state.total_transactions, 50);
    }

    #[test]
    fn test_block_advancement() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        let block_hash = [1u8; 32];
        manager.advance_block(block_hash, 5, Some(0x1d00fffe)).unwrap();

        let state = manager.get_state();
        assert_eq!(state.height, 1);
        assert_eq!(state.best_block_hash, block_hash);
        assert_eq!(state.total_transactions, 5);
        assert_eq!(state.difficulty_bits, 0x1d00fffe);
    }

    #[test]
    fn test_validator_management() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        // Add validator
        manager.update_validator(
            "validator1".to_string(),
            vec![1, 2, 3, 4],
            100,
            true,
        ).unwrap();

        let active_validators = manager.get_active_validators();
        assert_eq!(active_validators.len(), 1);
        assert_eq!(active_validators[0].0, "validator1");
        assert_eq!(active_validators[0].1.power, 100);

        // Remove validator
        manager.remove_validator("validator1").unwrap();
        let active_validators = manager.get_active_validators();
        assert_eq!(active_validators.len(), 0);
    }

    #[test]
    fn test_state_rollback() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        // Make changes
        manager.update_state(|state| {
            state.height = 10;
            Ok(())
        }).unwrap();

        assert_eq!(manager.get_state().height, 10);

        // Rollback
        manager.rollback().unwrap();
        assert_eq!(manager.get_state().height, 0);
    }

    #[test]
    fn test_state_validation() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        // Valid state
        assert!(manager.validate_state().is_ok());

        // Invalid state
        manager.update_state(|state| {
            state.height = 10;
            state.best_block_hash = [0; 32]; // Invalid: non-genesis with zero hash
            Ok(())
        }).unwrap();

        assert!(manager.validate_state().is_err());
    }

    #[test]
    fn test_state_export_import() {
        let initial_state = ConsensusState::default();
        let manager1 = StateManager::new(initial_state);

        // Update state
        manager1.update_state(|state| {
            state.height = 42;
            state.total_transactions = 100;
            Ok(())
        }).unwrap();

        // Export state
        let exported = manager1.export_state().unwrap();

        // Import to new manager
        let new_state = ConsensusState::default();
        let manager2 = StateManager::new(new_state);
        manager2.import_state(&exported).unwrap();

        // Verify
        let imported_state = manager2.get_state();
        assert_eq!(imported_state.height, 42);
        assert_eq!(imported_state.total_transactions, 100);
    }

    #[test]
    fn test_statistics() {
        let initial_state = ConsensusState::default();
        let manager = StateManager::new(initial_state);

        // Add some state
        manager.advance_block([1; 32], 10, None).unwrap();
        manager.update_validator("val1".to_string(), vec![1, 2], 100, true).unwrap();

        let stats = manager.get_statistics();
        assert_eq!(stats.current_height, 1);
        assert_eq!(stats.total_transactions, 10);
        assert_eq!(stats.active_validators, 1);
        assert_eq!(stats.total_validators, 1);
    }
}