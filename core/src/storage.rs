//! Blockchain storage layer usando RocksDB

use crate::{Block, Transaction, TxOutput, OutPoint};
use rocksdb::{DB, Options, ColumnFamily, ColumnFamilyDescriptor, WriteBatch};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

/// Column families per diversi tipi di dati
const CF_BLOCKS: &str = "blocks";           // block_hash -> Block
const CF_BLOCK_INDEX: &str = "block_index"; // height -> block_hash
const CF_UTXO: &str = "utxo";              // OutPoint -> TxOutput
const CF_METADATA: &str = "metadata";       // chiavi varie -> valori
const CF_TX_INDEX: &str = "tx_index";      // tx_hash -> (block_hash, tx_index)

/// Chiavi per metadata
const META_BEST_BLOCK: &str = "best_block_hash";
const META_HEIGHT: &str = "blockchain_height";
const META_TOTAL_WORK: &str = "total_work";
const META_GENESIS_HASH: &str = "genesis_hash";

/// Blockchain database manager
pub struct BlockchainDB {
    /// RocksDB instance
    db: Arc<DB>,
}

/// Informazioni su una transazione nell'indice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxLocation {
    /// Hash del block contenente la transazione
    pub block_hash: [u8; 32],
    /// Indice della transazione nel block
    pub tx_index: u32,
    /// Altezza del block
    pub block_height: u64,
}

/// Metadati della blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainMetadata {
    /// Hash del block migliore (tip)
    pub best_block_hash: [u8; 32],
    /// Altezza corrente della blockchain
    pub height: u64,
    /// Lavoro totale accumulato
    pub total_work: u64,
    /// Hash del genesis block
    pub genesis_hash: [u8; 32],
}

/// UTXO entry nel database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    /// Output della transazione
    pub output: TxOutput,
    /// Altezza del block in cui è stato creato
    pub block_height: u64,
    /// Se è un output coinbase (ha regole speciali)
    pub is_coinbase: bool,
}

impl BlockchainDB {
    /// Apre o crea un nuovo database blockchain
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Configurazioni per performance
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024);
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Definisci column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_BLOCKS, Options::default()),
            ColumnFamilyDescriptor::new(CF_BLOCK_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_UTXO, Options::default()),
            ColumnFamilyDescriptor::new(CF_METADATA, Options::default()),
            ColumnFamilyDescriptor::new(CF_TX_INDEX, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)
            .map_err(|e| StorageError::DatabaseOpen(e.to_string()))?;

        Ok(Self {
            db: Arc::new(db),
        })
    }

    /// Ottiene column family handle
    fn get_cf(&self, name: &str) -> Result<&ColumnFamily, StorageError> {
        self.db.cf_handle(name)
            .ok_or_else(|| StorageError::ColumnFamilyNotFound(name.to_string()))
    }

    /// Salva un nuovo block nella blockchain
    pub fn store_block(&self, block: &Block) -> Result<(), StorageError> {
        let mut batch = WriteBatch::default();
        let block_hash = block.hash();
        let height = block.header.height;

        // Serializza il block
        let block_bytes = bincode::serialize(block)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        // Salva block: hash -> block
        let blocks_cf = self.get_cf(CF_BLOCKS)?;
        batch.put_cf(blocks_cf, &block_hash, &block_bytes);

        // Salva indice altezza: height -> hash
        let index_cf = self.get_cf(CF_BLOCK_INDEX)?;
        batch.put_cf(index_cf, &height.to_be_bytes(), &block_hash);

        // Aggiorna UTXO set per ogni transazione
        for (tx_index, transaction) in block.transactions.iter().enumerate() {
            self.update_utxo_for_transaction(
                &mut batch,
                transaction,
                block_hash,
                height,
                tx_index as u32
            )?;
        }

        // Aggiorna metadati se questo è il nuovo best block
        self.update_best_block(&mut batch, block_hash, height)?;

        // Commit atomico
        self.db.write(batch)
            .map_err(|e| StorageError::Write(e.to_string()))?;

        Ok(())
    }

    /// Aggiorna UTXO set per una transazione
    fn update_utxo_for_transaction(
        &self,
        batch: &mut WriteBatch,
        tx: &Transaction,
        block_hash: [u8; 32],
        block_height: u64,
        tx_index: u32,
    ) -> Result<(), StorageError> {
        let utxo_cf = self.get_cf(CF_UTXO)?;
        let tx_cf = self.get_cf(CF_TX_INDEX)?;
        let tx_hash = tx.hash();

        // Salva indice transazione: tx_hash -> location
        let tx_location = TxLocation {
            block_hash,
            tx_index,
            block_height,
        };
        let location_bytes = bincode::serialize(&tx_location)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        batch.put_cf(tx_cf, &tx_hash, &location_bytes);

        // Rimuovi UTXO spesi (inputs)
        if !tx.is_coinbase() {
            for input in &tx.inputs {
                let outpoint_key = self.outpoint_key(&input.previous_output);
                batch.delete_cf(utxo_cf, &outpoint_key);
            }
        }

        // Aggiungi nuovi UTXO (outputs)
        for (vout, output) in tx.outputs.iter().enumerate() {
            let outpoint = OutPoint::new(tx_hash, vout as u32);
            let outpoint_key = self.outpoint_key(&outpoint);

            let utxo_entry = UtxoEntry {
                output: output.clone(),
                block_height,
                is_coinbase: tx.is_coinbase(),
            };

            let utxo_bytes = bincode::serialize(&utxo_entry)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;

            batch.put_cf(utxo_cf, &outpoint_key, &utxo_bytes);
        }

        Ok(())
    }

    /// Aggiorna il best block
    fn update_best_block(
        &self,
        batch: &mut WriteBatch,
        block_hash: [u8; 32],
        height: u64,
    ) -> Result<(), StorageError> {
        let metadata_cf = self.get_cf(CF_METADATA)?;

        batch.put_cf(metadata_cf, META_BEST_BLOCK, &block_hash);
        batch.put_cf(metadata_cf, META_HEIGHT, &height.to_be_bytes());

        Ok(())
    }

    /// Carica un block per hash
    pub fn get_block(&self, block_hash: &[u8; 32]) -> Result<Option<Block>, StorageError> {
        let blocks_cf = self.get_cf(CF_BLOCKS)?;

        match self.db.get_cf(blocks_cf, block_hash) {
            Ok(Some(block_bytes)) => {
                let block = bincode::deserialize(&block_bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(block))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Read(e.to_string())),
        }
    }

    /// Carica un block per altezza
    pub fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, StorageError> {
        let index_cf = self.get_cf(CF_BLOCK_INDEX)?;

        // Prima ottieni l'hash dalla height
        match self.db.get_cf(index_cf, &height.to_be_bytes()) {
            Ok(Some(hash_bytes)) => {
                if hash_bytes.len() == 32 {
                    let mut block_hash = [0u8; 32];
                    block_hash.copy_from_slice(&hash_bytes);
                    self.get_block(&block_hash)
                } else {
                    Err(StorageError::InvalidData("Invalid block hash length".to_string()))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Read(e.to_string())),
        }
    }

    /// Ottiene un UTXO
    pub fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<UtxoEntry>, StorageError> {
        let utxo_cf = self.get_cf(CF_UTXO)?;
        let key = self.outpoint_key(outpoint);

        match self.db.get_cf(utxo_cf, &key) {
            Ok(Some(utxo_bytes)) => {
                let utxo = bincode::deserialize(&utxo_bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;
                Ok(Some(utxo))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Read(e.to_string())),
        }
    }

    /// Verifica se un UTXO esiste ed è spendibile
    pub fn is_utxo_spendable(&self, outpoint: &OutPoint, current_height: u64) -> Result<bool, StorageError> {
        match self.get_utxo(outpoint)? {
            Some(utxo) => {
                // I coinbase output richiedono 100 blocchi di maturazione
                if utxo.is_coinbase {
                    let maturity_height = utxo.block_height + 100;
                    Ok(current_height >= maturity_height)
                } else {
                    Ok(true)
                }
            }
            None => Ok(false),
        }
    }

    /// Ottiene metadati della blockchain
    pub fn get_metadata(&self) -> Result<ChainMetadata, StorageError> {
        let metadata_cf = self.get_cf(CF_METADATA)?;

        // Best block hash
        let best_block_hash = self.db.get_cf(metadata_cf, META_BEST_BLOCK)
            .map_err(|e| StorageError::Read(e.to_string()))?
            .map(|bytes| {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&bytes[..32]);
                hash
            })
            .unwrap_or([0; 32]);

        // Height
        let height = self.db.get_cf(metadata_cf, META_HEIGHT)
            .map_err(|e| StorageError::Read(e.to_string()))?
            .map(|bytes| u64::from_be_bytes(bytes.try_into().unwrap_or([0; 8])))
            .unwrap_or(0);

        // Genesis hash
        let genesis_hash = self.db.get_cf(metadata_cf, META_GENESIS_HASH)
            .map_err(|e| StorageError::Read(e.to_string()))?
            .map(|bytes| {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&bytes[..32]);
                hash
            })
            .unwrap_or([0; 32]);

        Ok(ChainMetadata {
            best_block_hash,
            height,
            total_work: 0, // TODO: calcolare total work
            genesis_hash,
        })
    }

    /// Inizializza il database con il genesis block
    pub fn initialize_with_genesis(&self, genesis: &Block) -> Result<(), StorageError> {
        let metadata = self.get_metadata()?;

        // Se già inizializzato, non fare nulla
        if metadata.height > 0 {
            return Ok(());
        }

        let genesis_hash = genesis.hash();

        // Salva genesis block
        self.store_block(genesis)?;

        // Salva hash genesis nei metadati
        let metadata_cf = self.get_cf(CF_METADATA)?;
        let mut batch = WriteBatch::default();
        batch.put_cf(metadata_cf, META_GENESIS_HASH, &genesis_hash);

        self.db.write(batch)
            .map_err(|e| StorageError::Write(e.to_string()))?;

        Ok(())
    }

    /// Ottiene la height corrente della blockchain
    pub fn get_height(&self) -> Result<u64, StorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.height)
    }

    /// Ottiene l'hash del best block
    pub fn get_best_block_hash(&self) -> Result<[u8; 32], StorageError> {
        let metadata = self.get_metadata()?;
        Ok(metadata.best_block_hash)
    }

    /// Cerca una transazione per hash
    pub fn get_transaction(&self, tx_hash: &[u8; 32]) -> Result<Option<(Transaction, TxLocation)>, StorageError> {
        let tx_cf = self.get_cf(CF_TX_INDEX)?;

        // Prima cerca la location
        match self.db.get_cf(tx_cf, tx_hash) {
            Ok(Some(location_bytes)) => {
                let location: TxLocation = bincode::deserialize(&location_bytes)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?;

                // Carica il block
                if let Some(block) = self.get_block(&location.block_hash)? {
                    if let Some(tx) = block.transactions.get(location.tx_index as usize) {
                        return Ok(Some((tx.clone(), location)));
                    }
                }

                Err(StorageError::InvalidData("Transaction not found in referenced block".to_string()))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Read(e.to_string())),
        }
    }

    /// Crea chiave per OutPoint
    fn outpoint_key(&self, outpoint: &OutPoint) -> Vec<u8> {
        let mut key = Vec::with_capacity(36); // 32 + 4 bytes
        key.extend_from_slice(&outpoint.txid);
        key.extend_from_slice(&outpoint.vout.to_be_bytes());
        key
    }

    /// Ottiene statistiche del database
    pub fn get_stats(&self) -> Result<DatabaseStats, StorageError> {
        let metadata = self.get_metadata()?;

        // Count UTXO set size (approssimato)
        let utxo_cf = self.get_cf(CF_UTXO)?;
        let iter = self.db.iterator_cf(utxo_cf, rocksdb::IteratorMode::Start);
        let utxo_count = iter.count() as u64;

        Ok(DatabaseStats {
            height: metadata.height,
            best_block_hash: metadata.best_block_hash,
            utxo_set_size: utxo_count,
            total_blocks: metadata.height + 1, // +1 per genesis
        })
    }
}

/// Statistiche del database
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Altezza corrente
    pub height: u64,
    /// Hash best block
    pub best_block_hash: [u8; 32],
    /// Dimensione UTXO set
    pub utxo_set_size: u64,
    /// Numero totale di blocks
    pub total_blocks: u64,
}

/// Errori del storage
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Database open error: {0}")]
    DatabaseOpen(String),

    #[error("Column family not found: {0}")]
    ColumnFamilyNotFound(String),

    #[error("Read error: {0}")]
    Read(String),

    #[error("Write error: {0}")]
    Write(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Block not found: {hash:?}")]
    BlockNotFound { hash: [u8; 32] },

    #[error("UTXO not found: {outpoint:?}")]
    UtxoNotFound { outpoint: OutPoint },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (BlockchainDB, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = BlockchainDB::open(temp_dir.path()).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_database_creation() {
        let (db, _temp) = create_test_db();
        let metadata = db.get_metadata().unwrap();

        assert_eq!(metadata.height, 0);
        assert_eq!(metadata.best_block_hash, [0; 32]);
    }

    #[test]
    fn test_genesis_initialization() {
        let (db, _temp) = create_test_db();
        let genesis = Block::genesis();

        db.initialize_with_genesis(&genesis).unwrap();

        let metadata = db.get_metadata().unwrap();
        assert_eq!(metadata.height, 0);
        assert_eq!(metadata.genesis_hash, genesis.hash());

        // Verifica che il genesis sia salvato
        let stored_genesis = db.get_block_by_height(0).unwrap().unwrap();
        assert_eq!(stored_genesis.hash(), genesis.hash());
    }

    #[test]
    fn test_block_storage_retrieval() {
        let (db, _temp) = create_test_db();
        let genesis = Block::genesis();

        db.store_block(&genesis).unwrap();

        // Retrieval by hash
        let retrieved = db.get_block(&genesis.hash()).unwrap().unwrap();
        assert_eq!(retrieved.hash(), genesis.hash());

        // Retrieval by height
        let retrieved = db.get_block_by_height(0).unwrap().unwrap();
        assert_eq!(retrieved.hash(), genesis.hash());
    }

    #[test]
    fn test_utxo_management() {
        let (db, _temp) = create_test_db();

        // Crea block con transazione coinbase
        let coinbase = Transaction::coinbase(b"test_address", 0, 5000000000);
        let block = Block::new([0; 32], vec![coinbase.clone()], 0x1d00ffff, 0);

        db.store_block(&block).unwrap();

        // Verifica UTXO creation
        let outpoint = OutPoint::new(coinbase.hash(), 0);
        let utxo = db.get_utxo(&outpoint).unwrap();

        assert!(utxo.is_some());
        let utxo = utxo.unwrap();
        assert_eq!(utxo.output.value, 5000000000);
        assert!(utxo.is_coinbase);
    }

    #[test]
    fn test_transaction_indexing() {
        let (db, _temp) = create_test_db();

        let coinbase = Transaction::coinbase(b"test_address", 0, 5000000000);
        let tx_hash = coinbase.hash();
        let block = Block::new([0; 32], vec![coinbase], 0x1d00ffff, 0);

        db.store_block(&block).unwrap();

        // Cerca transazione
        let (tx, location) = db.get_transaction(&tx_hash).unwrap().unwrap();
        assert_eq!(tx.hash(), tx_hash);
        assert_eq!(location.block_hash, block.hash());
        assert_eq!(location.tx_index, 0);
    }

    #[test]
    fn test_coinbase_maturity() {
        let (db, _temp) = create_test_db();

        let coinbase = Transaction::coinbase(b"test_address", 0, 5000000000);
        let block = Block::new([0; 32], vec![coinbase.clone()], 0x1d00ffff, 0);

        db.store_block(&block).unwrap();

        let outpoint = OutPoint::new(coinbase.hash(), 0);

        // Non dovrebbe essere spendibile subito (height 0 < 100)
        assert!(!db.is_utxo_spendable(&outpoint, 50).unwrap());

        // Dovrebbe essere spendibile dopo 100 blocks
        assert!(db.is_utxo_spendable(&outpoint, 100).unwrap());
    }

    #[test]
    fn test_database_stats() {
        let (db, _temp) = create_test_db();
        let genesis = Block::genesis();

        db.store_block(&genesis).unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.height, 0);
        assert_eq!(stats.total_blocks, 1);
        assert!(stats.utxo_set_size >= 0); // Genesis potrebbe avere 0 UTXO
    }
}