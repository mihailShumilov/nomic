use bitcoin::PublicKey;
use orga::encoding::Result as EdResult;
use orga::encoding::{Decode, Encode};
use orga::macros::Entry;
use orga::prelude::Terminated;
use std::ops::{Deref, DerefMut};

pub struct PubKey(PublicKey);
impl Terminated for PubKey {}

impl Encode for PubKey {
    fn encode_into<W: std::io::Write>(&self, writer: &mut W) -> EdResult<()> {
        let bytes = self.0.to_bytes();
        Ok(writer.write_all(&bytes)?)
    }

    fn encoding_length(&self) -> EdResult<usize> {
        Ok(self.0.to_bytes().len())
    }
}

impl Decode for PubKey {
    fn decode<R: std::io::Read>(input: R) -> EdResult<Self> {
        Ok(PubKey(PublicKey::read_from(input)?))
    }
}

impl Deref for PubKey {
    type Target = PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PubKey {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<PublicKey> for PubKey {
    fn from(key: PublicKey) -> Self {
        PubKey(key)
    }
}

#[derive(Entry)]
pub struct Signatory {
    #[key]
    pub public_key: PubKey,
    pub voting_power: u64,
}

impl Signatory {
    pub fn new(public_key: PublicKey, voting_power: u64) -> Self {
        Signatory {
            public_key: PubKey(public_key),
            voting_power,
        }
    }

    pub fn voting_power(&self) -> u64 {
        //Thinking that there is real chance that this value could possibly be stale
        //However, theses signatories will only every be used in the context of the
        //bitcoin scripts, and when the checkpoints are made, the voting power only matters
        //for the current time the checkpoints is made
        //
        //
        //so the updates to voting power will only have to happen on the map that holds validator
        //address vs bitcoin public key
        //
        //so when we determine the signatory set, we can just look up the voting power of the
        //validator, hold that in memory for the duration of the function, look up the btc public
        //key that is associated with that nomic address, and then add that to the signatory set
        //with the new voting power
        //
        //then the checkpoint is constructed and the signatory set isn't used anywhere else
        //
        self.voting_power
    }
}
