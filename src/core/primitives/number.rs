use crate::Result;
use orga::{Decode, Encode};
use rust_decimal::Decimal;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};

pub struct Number(pub Decimal);

impl Deref for Number {
    type Target = Decimal;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Number {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Encode for Number {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
        let bytes = self.0.serialize();
        dest.write_all(&bytes)?;
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
        let n_str = n.to_string();
        assert_eq!(n_str, "9");
        let n = Number(n.checked_div(2.into()).unwrap());
        let n_bytes = n.encode().unwrap();
        let n = Number::decode(n_bytes.as_slice()).unwrap();
        assert_eq!(n.to_string(), "4.5");
    }
}
