# Sedly Blockchain

**Next-generation blockchain with hybrid eUTXO/EVM architecture**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-red.svg)](https://www.rust-lang.org)
[![Development Status](https://img.shields.io/badge/status-early%20development-orange.svg)]()

## Overview

Sedly is a decentralized blockchain platform that combines the best of Bitcoin's UTXO model with Ethereum's smart contract capabilities. Built from the ground up in Rust, Sedly offers a fair, transparent, and highly performant blockchain solution.

**⚠️ Early Development Notice:** Sedly is currently in active development. Core blockchain functionality is implemented, but user-facing applications (CLI, wallet, RPC) are not yet available.

### Key Features

- **Hybrid Architecture**: Extended UTXO (eUTXO) model with EVM compatibility (planned)
- **Fair Launch**: No premine, all coins generated through mining like Bitcoin
- **SHA-256 Mining**: Proven Proof-of-Work consensus with 2-minute block times
- **Multi-Asset Support**: Native support for multiple tokens on the same chain (planned)
- **High Performance**: Optimized for speed and scalability
- **Open Source**: Fully transparent development and governance

## Technical Specifications

| Specification | Value |
|---------------|-------|
| **Consensus Algorithm** | Proof of Work (SHA-256) |
| **Block Time** | 2 minutes |
| **Difficulty Adjustment** | Every 144 blocks (~4.8 hours) |
| **Initial Block Reward** | 50 SLY |
| **Halving Interval** | 210,000 blocks (~1 year) |
| **Max Supply** | 21,000,000 SLY |
| **Address Format** | Native Sedly addresses (in development) |

## Current Implementation Status

### ✅ Completed Components

- **Core Blockchain Engine**: Complete eUTXO transaction model
- **Mining Engine**: SHA-256 mining with multi-threading support
- **Difficulty Adjustment**: Dynamic difficulty adjustment every 144 blocks
- **Storage Layer**: RocksDB-based persistent storage with UTXO set management
- **Block Validation**: Complete block and transaction validation
- **State Management**: Consensus state management framework

### 🚧 In Development

- **Consensus Integration**: Tendermint ABCI integration (dependency issues being resolved)
- **P2P Networking**: Node-to-node communication protocol
- **RPC API**: JSON-RPC interface for blockchain queries

### 📋 Planned Features

- **CLI Applications**: Command-line node and mining software
- **Wallet Functionality**: Key management and transaction building
- **Smart Contracts**: EVM compatibility layer
- **Multi-Asset Support**: Native token support

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Sedly Blockchain                         │
├─────────────────────┬───────────────────┬───────────────────┤
│   Application Layer │   Consensus Layer │   Network Layer   │
│     (Planned)       │  (In Development) │    (Planned)      │
│  • Smart Contracts  │  • State Machine  │  • P2P Protocol   │
│  • DApps            │  • Validation     │  • Peer Discovery │
│  • Wallets          │  • Block Building │  • Gossip Network │
└─────────────────────┼───────────────────┼───────────────────┘
                      │                   │
┌─────────────────────┼───────────────────┼───────────────────┐
│              Core Blockchain Engine (✅ Complete)           │
├─────────────────────────────────────────────────────────────┤
│  • eUTXO Transaction Model                                  │
│  • SHA-256 Mining & Difficulty Adjustment                   │
│  • RocksDB Storage Layer                                    │
│  • Block/Transaction Validation                             │
└─────────────────────────────────────────────────────────────┘
```

## Getting Started

### Prerequisites

- **Rust**: 1.70 or higher
- **LLVM**: Required for RocksDB compilation
- **Git**: For repository cloning

### Installation

#### Windows
```powershell
# Install LLVM
winget install LLVM.LLVM

# Clone repository
git clone https://github.com/sedly-official/sedly-blockchain.git
cd sedly-blockchain

# Build project
cargo build --release
```

#### Linux/macOS
```bash
# Install dependencies (Ubuntu/Debian)
sudo apt update
sudo apt install build-essential clang llvm-dev

# Clone repository
git clone https://github.com/sedly-official/sedly-blockchain.git
cd sedly-blockchain

# Build project
cargo build --release
```

## Current Usage

### Running Tests
```bash
# Test core blockchain functionality
cargo test --workspace

# Test specific modules
cargo test core
cargo test consensus
```

### Development Tools
```bash
# Format code
cargo fmt

# Lint code
cargo clippy

# Generate documentation
cargo doc --open

# Watch for changes and rebuild
cargo install cargo-watch
cargo watch -x check
```

## Project Structure

```
sedly-blockchain/
├── core/                 # ✅ Core blockchain logic (Complete)
│   ├── src/
│   │   ├── block.rs     # Block structures and validation
│   │   ├── transaction.rs # eUTXO transaction model
│   │   ├── mining.rs    # SHA-256 mining engine
│   │   ├── difficulty.rs # Difficulty adjustment algorithm
│   │   ├── storage.rs   # RocksDB storage layer
│   │   ├── validation.rs # Transaction/block validation
│   │   └── lib.rs       # Core module exports
│   └── Cargo.toml
├── consensus/            # 🚧 State management (Partial)
│   ├── src/
│   │   ├── state.rs     # Consensus state management
│   │   └── lib.rs       # Module exports
│   └── Cargo.toml
├── docs/                 # 📋 Documentation (Planned)
├── scripts/              # 📋 Utility scripts (Planned)
├── README.md
└── Cargo.toml           # Workspace configuration
```

## Development

### Core Library Usage

```rust
use sedly_core::{Block, Transaction, BlockchainDB, Miner};

// Initialize database
let db = BlockchainDB::open("./blockchain_data")?;

// Create genesis block
let genesis = Block::genesis();
db.initialize_with_genesis(&genesis)?;

// Initialize miner
let target = [0x0f; 32]; // Easy target for testing
let miner = Miner::new(target, 2); // 2 threads

// Mine a block
let transactions = vec![Transaction::genesis()];
let result = miner.mine_block([0; 32], transactions, 1, 0x1d00ffff)?;

// Store block
db.store_block(&result.block)?;

// Query blockchain
let stored_block = db.get_block_by_height(0)?.unwrap();
println!("Genesis block hash: {:?}", hex::encode(stored_block.hash()));
```

### Testing the Mining Engine

```rust
use sedly_core::{Miner, Transaction};

#[test]
fn test_mining_example() {
    let target = [0x0f; 32]; // Easy target
    let miner = Miner::new(target, 1);
    let transactions = vec![Transaction::genesis()];
    
    let result = miner.mine_block([0; 32], transactions, 1, 0x1d00ffff).unwrap();
    
    println!("Mined block in {} hashes", result.hashes_calculated);
    println!("Hash rate: {:.2} H/s", result.hash_rate);
    assert!(result.block.header.meets_difficulty());
}
```

## Roadmap

### Phase 1: Core Foundation (Partially Complete)
- [x] eUTXO transaction model
- [x] SHA-256 mining engine with multi-threading
- [x] Difficulty adjustment algorithm (every 144 blocks)
- [x] RocksDB storage layer with UTXO management
- [x] Block and transaction validation
- [x] Consensus state management framework
- [ ] Dependency resolution for full consensus integration

### Phase 2: Consensus & Networking (Next Priority)
- [ ] Complete Tendermint ABCI integration
- [ ] P2P networking protocol implementation
- [ ] Node discovery and peer management
- [ ] Transaction pool and mempool management
- [ ] Block synchronization and validation

### Phase 3: User Interface (Future)
- [ ] CLI node application
- [ ] Mining software with pool support
- [ ] Wallet functionality (key generation, transactions)
- [ ] JSON-RPC API server
- [ ] Web-based blockchain explorer

### Phase 4: Advanced Features (Future)
- [ ] Smart contract support (EVM compatibility)
- [ ] Multi-asset transactions
- [ ] Cross-chain bridges
- [ ] Governance mechanisms
- [ ] Mobile and web wallets

## Contributing

We welcome contributions from the community!

### Development Workflow
1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and add tests
4. Ensure all tests pass: `cargo test --workspace`
5. Format your code: `cargo fmt`
6. Submit a pull request

### Current Development Priorities
1. **Resolve dependency issues** for Tendermint integration
2. **Implement P2P networking** layer
3. **Create CLI applications** for node and mining
4. **Add comprehensive testing** for all components
5. **Improve documentation** and examples

### Reporting Issues
- Use GitHub Issues for bug reports and feature requests
- Provide detailed information about your environment
- Include steps to reproduce any bugs

## Community

- **Website**: [sedly.it](https://sedly.it)
- **GitHub**: [github.com/sedly-official/sedly-blockchain](https://github.com/sedly-official/sedly-blockchain)
- **Discord**: Coming soon
- **Telegram**: Coming soon
- **Twitter**: Coming soon

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- **Bitcoin**: For pioneering cryptocurrency and the UTXO model
- **Ethereum**: For smart contract innovation
- **Cardano**: For extended UTXO concepts
- **Tendermint**: For BFT consensus algorithms
- **Rust Community**: For the amazing language and ecosystem

## Disclaimer

Sedly is experimental software under active development. The current implementation is a development prototype and should not be used in production environments. The developers are not responsible for any loss of funds or other damages.

---

**Built with ❤️ in Rust**

For developers interested in contributing, start by exploring the `core` module which contains the complete blockchain implementation.