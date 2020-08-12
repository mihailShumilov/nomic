use crate::Result;
use orga::{Decode, Encode};
use rust_decimal::Decimal;
use std::io::{Read, Write};
pub struct Number(Decimal);

impl Encode for Number {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
        let bytes = self.0.serialize();
        dest.write(&bytes)?;
        Ok(())
    }

    fn encoding_length(&self) -> Result<usize> {
        Ok(16)
    }
}

impl Decode for Number {
    fn decode<R: Read>(mut input: R) -> Result<Self> {
        let mut bytes = [0; 16];
        input.read_exact(&mut bytes)?;
        Ok(Self(Decimal::deserialize(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encode_decode() {
        let n = Number(9.into());
        let n_str = n.0.to_string();
        assert_eq!(n_str, "9");
        let n = Number(n.0.checked_div(2.into()).unwrap());
        let n_bytes = n.encode().unwrap();
        let n = Number::decode(n_bytes.as_slice()).unwrap();
        assert_eq!(n.0.to_string(), "4.5");
    }
}
