use crate::core::primitives::{Address, Number};
use crate::Result;
use failure::bail;
use orga::encoding::{self as ed, Decode, Encode};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Direction {
    Long,
    Short,
}

impl Encode for Direction {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
        let bytes = match self {
            Direction::Long => &[0],
            Direction::Short => &[1],
        };
        dest.write_all(bytes)?;
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        Ok(1)
    }
}

impl Decode for Direction {
    fn decode<R: Read>(input: R) -> Result<Self> {
        Ok(match input.bytes().next() {
            Some(Ok(0)) => Direction::Long,
            Some(Ok(1)) => Direction::Short,
            Some(Err(err)) => Err(err)?,
            None => bail!("EOF"),
            _ => bail!("Failed to decode Direction"),
        })
    }
}

pub struct PositionState;

#[derive(Encode, Decode)]
pub struct PositionValue {
    pub direction: Direction,
    pub size: u64,
    pub entry_price: Number,
}

pub struct Position {
    pub direction: Direction,
    pub size: u64,
    pub entry_price: Number,
    pub address: Address,
}

// impl PositionState {
//     pub fn by_address(address: &Address) -> Result<Position> {}

//     /// Given a new mark price, clear all liquidated positions from the collection and return them.
//     pub fn liquidate(mark_price: Number) -> Result<Vec<Position>> {}
// }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        assert_eq!(2, 2);
    }
}
