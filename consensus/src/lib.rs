//! Sedly Consensus - Tendermint ABCI integration

pub mod abci;
pub mod server;
pub mod state;

pub use abci::{SedlyApp, ConsensusError};
pub use server::{ConsensusServer, ServerConfig};
pub use state::{ConsensusState, StateManager};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}