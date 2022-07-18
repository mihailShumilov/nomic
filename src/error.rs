#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Bitcoin(#[from] bitcoin::Error),
    #[error(transparent)]
    BitcoinHash(#[from] bitcoin::hashes::Error),
    #[error(transparent)]
    BitcoinEncode(#[from] bitcoin::consensus::encode::Error),
    #[error(transparent)]
    Bip32(#[from] bitcoin::util::bip32::Error),
    #[error(transparent)]
    Sighash(#[from] bitcoin::util::sighash::Error),
    #[error(transparent)]
    TryFrom(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    Secp(#[from] bitcoin::secp256k1::Error),
    #[error("Could not verify merkle proof")]
    BitcoinMerkleBlockError,
    #[cfg(feature = "full")]
    #[error(transparent)]
    BitcoinRpc(#[from] bitcoincore_rpc_async::Error),
    #[error("{0}")]
    Header(String),
    #[error(transparent)]
    Orga(#[from] orga::Error),
    #[error("{0}")]
    Relayer(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Unknown Error")]
    Unknown,
}

pub type Result<T> = std::result::Result<T, Error>;
