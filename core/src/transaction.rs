//! eUTXO Transaction structures per Sedly blockchain

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Transazione eUTXO (extended UTXO)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Versione del formato transazione
    pub version: u32,
    /// Input della transazione (UTXO spesi)
    pub inputs: Vec<TxInput>,
    /// Output della transazione (nuovi UTXO creati)
    pub outputs: Vec<TxOutput>,
    /// Lock time (0 = valida subito)
    pub lock_time: u64,
}

/// Input di transazione (riferimento a UTXO esistente)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxInput {
    /// Riferimento all'output precedente da spendere
    pub previous_output: OutPoint,
    /// Script per sbloccare l'UTXO (firma + pubkey)
    pub script_sig: Vec<u8>,
    /// Numero di sequenza (per timelock avanzati)
    pub sequence: u32,
}

/// Output di transazione (nuovo UTXO creato)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxOutput {
    /// Valore in satoshi (1 SLY = 100,000,000 satoshi)
    pub value: u64,
    /// Asset ID (per multi-asset, [0;32] = native SLY)
    pub asset_id: [u8; 32],
    /// Script che definisce come spendere questo output
    pub script_pubkey: Vec<u8>,
}

/// Riferimento a un output di transazione precedente
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutPoint {
    /// Hash della transazione che contiene l'output
    pub txid: [u8; 32],
    /// Indice dell'output nella transazione (0, 1, 2...)
    pub vout: u32,
}

/// Tipo di transazione
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionType {
    /// Transazione Coinbase (mining reward)
    Coinbase,
    /// Transazione normale
    Regular,
}

impl Transaction {
    /// Crea nuova transazione
    pub fn new(
        inputs: Vec<TxInput>,
        outputs: Vec<TxOutput>,
        lock_time: u64,
    ) -> Self {
        Self {
            version: crate::PROTOCOL_VERSION,
            inputs,
            outputs,
            lock_time,
        }
    }

    /// Calcola hash della transazione (double SHA-256)
    pub fn hash(&self) -> [u8; 32] {
        let tx_bytes = bincode::serialize(self)
            .expect("Failed to serialize transaction");

        // Double SHA-256 come Bitcoin
        let hash1 = Sha256::digest(&tx_bytes);
        let hash2 = Sha256::digest(&hash1);

        hash2.into()
    }

    /// Verifica se è una transazione coinbase
    pub fn is_coinbase(&self) -> bool {
        self.inputs.len() == 1 &&
            self.inputs[0].previous_output.txid == [0; 32] &&
            self.inputs[0].previous_output.vout == 0xffffffff
    }

    /// Ottiene il tipo di transazione
    pub fn transaction_type(&self) -> TransactionType {
        if self.is_coinbase() {
            TransactionType::Coinbase
        } else {
            TransactionType::Regular
        }
    }

    /// Crea transazione coinbase per mining reward
    pub fn coinbase(reward_address: &[u8], block_height: u64, reward: u64) -> Self {
        // Input coinbase (speciale)
        let coinbase_input = TxInput {
            previous_output: OutPoint {
                txid: [0; 32],
                vout: 0xffffffff,
            },
            script_sig: Self::create_coinbase_script(block_height),
            sequence: 0xffffffff,
        };

        // Output con reward
        let reward_output = TxOutput {
            value: reward,
            asset_id: [0; 32], // Native SLY asset
            script_pubkey: reward_address.to_vec(),
        };

        Self::new(
            vec![coinbase_input],
            vec![reward_output],
            0,
        )
    }

    /// Crea script coinbase con block height
    fn create_coinbase_script(block_height: u64) -> Vec<u8> {
        let mut script = Vec::new();

        // Aggiungi block height (BIP34)
        let height_bytes = block_height.to_le_bytes();
        script.push(height_bytes.len() as u8);
        script.extend_from_slice(&height_bytes);

        // Aggiungi timestamp
        script.extend_from_slice(b"Sedly Genesis");

        script
    }

    /// Crea transazione genesis (prima transazione della blockchain)
    pub fn genesis() -> Self {
        let genesis_message = b"Sedly - Fair Launch Blockchain";

        let coinbase_input = TxInput {
            previous_output: OutPoint {
                txid: [0; 32],
                vout: 0xffffffff,
            },
            script_sig: genesis_message.to_vec(),
            sequence: 0xffffffff,
        };

        // Genesis non ha output (tutto il supply viene creato tramite mining)
        Self::new(
            vec![coinbase_input],
            vec![],
            0,
        )
    }

    /// Calcola total input value
    pub fn input_value(&self) -> u64 {
        // TODO: Implementare lookup UTXO set per calcolare valore reale
        // Per ora ritorna 0 per coinbase, altrimenti richiede UTXO set
        if self.is_coinbase() {
            0
        } else {
            // Richiede accesso al UTXO set per calcolare
            0
        }
    }

    /// Calcola total output value
    pub fn output_value(&self) -> u64 {
        self.outputs.iter()
            .map(|output| output.value)
            .sum()
    }

    /// Calcola fee della transazione
    pub fn fee(&self) -> u64 {
        if self.is_coinbase() {
            0
        } else {
            // fee = input_value - output_value
            let input_val = self.input_value();
            let output_val = self.output_value();

            if input_val >= output_val {
                input_val - output_val
            } else {
                0 // Transazione invalida
            }
        }
    }

    /// Verifica validità base della transazione
    pub fn is_valid(&self) -> bool {
        // Verifica che abbia almeno un input e un output (eccetto genesis)
        if self.inputs.is_empty() {
            return false;
        }

        // Verifica che gli output non siano vuoti per transazioni normali
        if !self.is_coinbase() && self.outputs.is_empty() {
            return false;
        }

        // Verifica che i valori degli output siano positivi
        for output in &self.outputs {
            if output.value == 0 {
                return false;
            }
        }

        // TODO: Verifica firme e script

        true
    }

    /// Dimensione della transazione in bytes
    pub fn size(&self) -> usize {
        bincode::serialize(self)
            .map(|bytes| bytes.len())
            .unwrap_or(0)
    }
}

impl TxOutput {
    /// Crea nuovo output
    pub fn new(value: u64, asset_id: [u8; 32], script_pubkey: Vec<u8>) -> Self {
        Self {
            value,
            asset_id,
            script_pubkey,
        }
    }

    /// Crea output per indirizzo standard (P2PKH-style)
    pub fn to_address(value: u64, address: &[u8]) -> Self {
        Self::new(
            value,
            [0; 32], // Native SLY asset
            address.to_vec(),
        )
    }

    /// Verifica se è un output nativo SLY
    pub fn is_native_asset(&self) -> bool {
        self.asset_id == [0; 32]
    }
}

impl OutPoint {
    /// Crea nuovo OutPoint
    pub fn new(txid: [u8; 32], vout: u32) -> Self {
        Self { txid, vout }
    }

    /// Verifica se è un OutPoint nullo (coinbase)
    pub fn is_null(&self) -> bool {
        self.txid == [0; 32] && self.vout == 0xffffffff
    }
}

impl TxInput {
    /// Crea nuovo input
    pub fn new(previous_output: OutPoint, script_sig: Vec<u8>) -> Self {
        Self {
            previous_output,
            script_sig,
            sequence: 0xffffffff,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_hash() {
        let tx = Transaction::genesis();
        let hash = tx.hash();

        assert_eq!(hash.len(), 32);
        assert_ne!(hash, [0; 32]);
    }

    #[test]
    fn test_genesis_transaction() {
        let genesis = Transaction::genesis();

        assert!(genesis.is_coinbase());
        assert_eq!(genesis.transaction_type(), TransactionType::Coinbase);
        assert_eq!(genesis.inputs.len(), 1);
        assert_eq!(genesis.outputs.len(), 0);
    }

    #[test]
    fn test_coinbase_transaction() {
        let reward_address = b"sedly1test_address";
        let coinbase = Transaction::coinbase(reward_address, 1, crate::INITIAL_BLOCK_REWARD);

        assert!(coinbase.is_coinbase());
        assert_eq!(coinbase.outputs.len(), 1);
        assert_eq!(coinbase.outputs[0].value, crate::INITIAL_BLOCK_REWARD);
    }

    #[test]
    fn test_outpoint_null() {
        let null_outpoint = OutPoint::new([0; 32], 0xffffffff);
        assert!(null_outpoint.is_null());

        let normal_outpoint = OutPoint::new([1; 32], 0);
        assert!(!normal_outpoint.is_null());
    }

    #[test]
    fn test_output_native_asset() {
        let output = TxOutput::to_address(1000, b"test_address");
        assert!(output.is_native_asset());
        assert_eq!(output.value, 1000);
    }
}