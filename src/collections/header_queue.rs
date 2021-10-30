use bitcoin::blockdata::block::BlockHeader;
use bitcoin::consensus::{Decodable, Encodable};
use orga::collections::Deque;
use orga::encoding::Result as EncodingResult;
use orga::prelude::*;
use orga::state::State;
use orga::store::Store;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct HeaderAdapter(BlockHeader);

//need to make sure that this doesn't cause any issues after the state is reset from the store
impl Default for HeaderAdapter {
    fn default() -> Self {
        HeaderAdapter(BlockHeader {
            version: Default::default(),
            prev_blockhash: Default::default(),
            merkle_root: Default::default(),
            time: Default::default(),
            bits: Default::default(),
            nonce: Default::default(),
        })
    }
}

impl State for HeaderAdapter {
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> orga::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> orga::Result<Self::Encoding> {
        Ok(self)
    }
}

impl Deref for HeaderAdapter {
    type Target = BlockHeader;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HeaderAdapter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Encode for HeaderAdapter {
    fn encode(&self) -> EncodingResult<Vec<u8>> {
        let mut dest: Vec<u8> = Vec::new();
        self.encode_into(&mut dest)?;
        Ok(dest)
    }

    fn encode_into<W: Write>(&self, dest: &mut W) -> EncodingResult<()> {
        match self.0.consensus_encode(dest) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn encoding_length(&self) -> EncodingResult<usize> {
        let mut _dest: Vec<u8> = Vec::new();
        match self.0.consensus_encode(_dest) {
            Ok(inner) => Ok(inner),
            Err(e) => Err(e.into()),
        }
    }
}

impl Decode for HeaderAdapter {
    fn decode<R: Read>(input: R) -> EncodingResult<Self> {
        let decoded_bytes = Decodable::consensus_decode(input);
        match decoded_bytes {
            Ok(inner) => Ok(Self(inner)),
            Err(_) => {
                let std_e = std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to decode bitcoin primitive",
                );
                Err(std_e.into())
            }
        }
    }
}

#[derive(Clone)]
pub struct Uint256(bitcoin::util::uint::Uint256);

impl Default for Uint256 {
    fn default() -> Self {
        Uint256(Default::default())
    }
}

impl Terminated for Uint256 {}

impl From<bitcoin::util::uint::Uint256> for Uint256 {
    fn from(value: bitcoin::util::uint::Uint256) -> Self {
        Uint256(value)
    }
}

impl Add for Uint256 {
    type Output = Uint256;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Uint256 {
    fn add_assign(&mut self, rhs: Self) {
        *self = Self(self.0 + rhs.0);
    }
}
impl Sub for Uint256 {
    type Output = Uint256;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Uint256 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = Self(self.0 - rhs.0);
    }
}

impl PartialEq for Uint256 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl PartialOrd for Uint256 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        bitcoin::util::uint::Uint256::partial_cmp(&self.0, &other.0)
    }
}

impl Encode for Uint256 {
    fn encode(&self) -> EncodingResult<Vec<u8>> {
        let mut dest: Vec<u8> = Vec::new();
        self.encode_into(&mut dest)?;
        Ok(dest)
    }

    fn encode_into<W: Write>(&self, dest: &mut W) -> EncodingResult<()> {
        match self.0.consensus_encode(dest) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn encoding_length(&self) -> EncodingResult<usize> {
        let mut _dest: Vec<u8> = Vec::new();
        match self.0.consensus_encode(_dest) {
            Ok(inner) => Ok(inner),
            Err(e) => Err(e.into()),
        }
    }
}

impl Decode for Uint256 {
    fn decode<R: Read>(input: R) -> EncodingResult<Self> {
        let decoded_bytes = Decodable::consensus_decode(input);
        match decoded_bytes {
            Ok(inner) => Ok(Self(inner)),
            Err(_) => {
                let std_e = std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to decode bitcoin primitive",
                );
                Err(std_e.into())
            }
        }
    }
}

impl State for Uint256 {
    type Encoding = Self;

    fn create(_: Store, data: Self::Encoding) -> orga::Result<Self> {
        Ok(data)
    }

    fn flush(self) -> orga::Result<Self::Encoding> {
        Ok(self)
    }
}

#[derive(Clone, State)]
pub struct WrappedHeader {
    height: u32,
    header: HeaderAdapter,
}

#[derive(State)]
pub struct HeaderQueue {
    inner: Deque<WrappedHeader>,
}

impl HeaderQueue {
    fn add<T>(&mut self, headers: T) -> Result<()>
    where
        T: IntoIterator<Item = WrappedHeader>,
    {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::BlockHash;
    use bitcoin_hashes::hex::FromHex;
    use bitcoin_hashes::sha256d::Hash;
    use chrono::{TimeZone, Utc};

    #[test]
    fn primitive_adapter_encode_decode() {
        let stamp = Utc.ymd(2009, 1, 10).and_hms(11, 39, 0);

        //Bitcoin block 42
        let header = BlockHeader {
            version: 1,
            prev_blockhash: BlockHash::from_hash(
                Hash::from_hex("00000000ad2b48c7032b6d7d4f2e19e54d79b1c159f5599056492f2cd7bb528b")
                    .unwrap(),
            ),
            merkle_root: "27c4d937dca276fb2b61e579902e8a876fd5b5abc17590410ced02d5a9f8e483"
                .parse()
                .unwrap(),
            time: stamp.timestamp() as u32,
            bits: 486_604_799,
            nonce: 3_600_650_283,
        };

        let adapter = HeaderAdapter(header);
        let encoded_adapter = adapter.encode().unwrap();

        let decoded_adapter: HeaderAdapter = Decode::decode(encoded_adapter.as_slice()).unwrap();

        assert_eq!(*decoded_adapter, header);
    }

    #[test]
    fn add_into_iterator() {
        let stamp = Utc.ymd(2009, 1, 10).and_hms(11, 39, 0);

        let header = BlockHeader {
            version: 1,
            prev_blockhash: BlockHash::from_hash(
                Hash::from_hex("00000000ad2b48c7032b6d7d4f2e19e54d79b1c159f5599056492f2cd7bb528b")
                    .unwrap(),
            ),
            merkle_root: "27c4d937dca276fb2b61e579902e8a876fd5b5abc17590410ced02d5a9f8e483"
                .parse()
                .unwrap(),
            time: stamp.timestamp() as u32,
            bits: 486_604_799,
            nonce: 3_600_650_283,
        };

        let adapter = HeaderAdapter(header);

        let header_list = [WrappedHeader {
            height: 1,
            header: adapter,
        }];

        let store = Store::new(Shared::new(MapStore::new()));
        let mut q = HeaderQueue::create(store, Default::default()).unwrap();
        q.add(header_list).unwrap();

        let adapter = HeaderAdapter(header);

        let header_list = vec![WrappedHeader {
            height: 1,
            header: adapter,
        }];

        let store = Store::new(Shared::new(MapStore::new()));
        let mut q = HeaderQueue::create(store, Default::default()).unwrap();
        q.add(header_list).unwrap();
    }
}
