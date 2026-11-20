//! Bit-level packet writer and reader.
//!
//! Packets are packed bit by bit, least significant bit first, so a boolean
//! costs one bit and a stance two. Floats go either raw (32 bits, bit-exact,
//! for state that prediction compares) or quantized to a fixed step.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The packet ended before a field was complete.
    Truncated,
    /// A field held a value that no valid packet contains.
    Invalid(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Truncated => f.write_str("packet truncated"),
            DecodeError::Invalid(what) => write!(f, "invalid {what}"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub type DecodeResult<T> = Result<T, DecodeError>;

#[derive(Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    /// Bits used in the last byte (0 means the next bit starts a new byte).
    used: u32,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes the low `count` bits of `value` (`count` ≤ 32).
    pub fn put(&mut self, value: u32, count: u32) {
        debug_assert!(count <= 32);
        debug_assert!(
            count == 32 || value >> count == 0,
            "{value} needs more than {count} bits"
        );
        let mut value = u64::from(value);
        let mut left = count;
        while left > 0 {
            if self.used == 0 {
                self.bytes.push(0);
            }
            let free = 8 - self.used;
            let take = free.min(left);
            let last = self.bytes.last_mut().expect("a byte was pushed");
            *last |= ((value & ((1 << take) - 1)) as u8) << self.used;
            value >>= take;
            left -= take;
            self.used = (self.used + take) % 8;
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub fn len_bits(&self) -> usize {
        if self.used == 0 {
            self.bytes.len() * 8
        } else {
            (self.bytes.len() - 1) * 8 + self.used as usize
        }
    }
}

pub struct BitReader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit: 0 }
    }

    pub fn take(&mut self, count: u32) -> DecodeResult<u32> {
        debug_assert!(count <= 32);
        if self.bit + count as usize > self.bytes.len() * 8 {
            return Err(DecodeError::Truncated);
        }
        let mut value = 0u64;
        let mut got = 0;
        while got < count {
            let byte = self.bytes[self.bit / 8];
            let offset = (self.bit % 8) as u32;
            let take = (8 - offset).min(count - got);
            let chunk = (u64::from(byte) >> offset) & ((1 << take) - 1);
            value |= chunk << got;
            got += take;
            self.bit += take as usize;
        }
        Ok(value as u32)
    }

    /// True when only zero padding bits remain.
    pub fn is_done(&self) -> bool {
        self.bytes.len() * 8 - self.bit < 8
    }
}

/// Where encoders write fields: a packet ([`BitWriter`]), or a recorder or
/// delta coder (`net::delta`). Every field is some number of bits, so an
/// encoder written against this trait can be delta-compressed field by field
/// without knowing it.
pub trait BitSink {
    /// Writes the low `count` bits of `value` (`count` ≤ 32) as one field.
    fn bits(&mut self, value: u32, count: u32);

    /// Like [`BitSink::bits`], for a field that is a count or a position
    /// rather than a bit pattern: a delta coder sends its change as a small
    /// signed difference instead of a bitwise XOR.
    fn number(&mut self, value: u32, count: u32) {
        self.bits(value, count);
    }

    fn bool(&mut self, value: bool) {
        self.bits(u32::from(value), 1);
    }

    fn u8(&mut self, value: u8) {
        self.bits(u32::from(value), 8);
    }

    fn u16(&mut self, value: u16) {
        self.bits(u32::from(value), 16);
    }

    fn u32(&mut self, value: u32) {
        self.bits(value, 32);
    }

    /// Bit-exact float.
    fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    /// Unsigned LEB128-style variable-length integer in 7-bit groups: small values are cheap.
    fn varint(&mut self, mut value: u32) {
        loop {
            let group = value & 0x7F;
            value >>= 7;
            self.bits(group, 7);
            self.bool(value != 0);
            if value == 0 {
                break;
            }
        }
    }

    /// Signed varint with zigzag encoding, so small negative numbers stay small.
    fn varint_signed(&mut self, value: i32) {
        self.varint(((value << 1) ^ (value >> 31)) as u32);
    }
}

/// Where decoders read fields from; the mirror of [`BitSink`].
pub trait BitSource {
    /// Reads one field of `count` bits (`count` ≤ 32).
    fn bits(&mut self, count: u32) -> DecodeResult<u32>;

    /// Reads a field written with [`BitSink::number`].
    fn number(&mut self, count: u32) -> DecodeResult<u32> {
        self.bits(count)
    }

    fn bool(&mut self) -> DecodeResult<bool> {
        Ok(self.bits(1)? == 1)
    }

    fn u8(&mut self) -> DecodeResult<u8> {
        Ok(self.bits(8)? as u8)
    }

    fn u16(&mut self) -> DecodeResult<u16> {
        Ok(self.bits(16)? as u16)
    }

    fn u32(&mut self) -> DecodeResult<u32> {
        self.bits(32)
    }

    fn f32(&mut self) -> DecodeResult<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn varint(&mut self) -> DecodeResult<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let group = self.bits(7)?;
            if shift == 28 && group > 0xF {
                return Err(DecodeError::Invalid("varint"));
            }
            value |= group << shift;
            if !self.bool()? {
                return Ok(value);
            }
        }
        Err(DecodeError::Invalid("varint"))
    }

    fn varint_signed(&mut self) -> DecodeResult<i32> {
        let raw = self.varint()?;
        Ok(((raw >> 1) as i32) ^ -((raw & 1) as i32))
    }
}

impl BitSink for BitWriter {
    fn bits(&mut self, value: u32, count: u32) {
        self.put(value, count);
    }
}

impl BitSource for BitReader<'_> {
    fn bits(&mut self, count: u32) -> DecodeResult<u32> {
        self.take(count)
    }
}

/// Maps `value` in `[min, max]` to an integer with `bits` bits of precision.
pub fn quantize(value: f32, min: f32, max: f32, bits: u32) -> u32 {
    let steps = ((1u64 << bits) - 1) as f32;
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    (t * steps).round() as u32
}

pub fn dequantize(value: u32, min: f32, max: f32, bits: u32) -> f32 {
    let steps = ((1u64 << bits) - 1) as f32;
    min + (max - min) * (value as f32 / steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_mixed_fields() {
        let mut w = BitWriter::new();
        w.bool(true);
        w.bits(5, 3);
        w.u16(0xBEEF);
        w.f32(-1.25);
        w.varint(300);
        w.varint_signed(-7);
        w.varint(u32::MAX);
        w.varint_signed(i32::MIN);
        w.u32(0xDEAD_BEEF);
        let bytes = w.finish();

        let mut r = BitReader::new(&bytes);
        assert!(r.bool().unwrap());
        assert_eq!(r.bits(3).unwrap(), 5);
        assert_eq!(r.u16().unwrap(), 0xBEEF);
        assert_eq!(r.f32().unwrap(), -1.25);
        assert_eq!(r.varint().unwrap(), 300);
        assert_eq!(r.varint_signed().unwrap(), -7);
        assert_eq!(r.varint().unwrap(), u32::MAX);
        assert_eq!(r.varint_signed().unwrap(), i32::MIN);
        assert_eq!(r.u32().unwrap(), 0xDEAD_BEEF);
        assert!(r.is_done());
    }

    #[test]
    fn booleans_pack_into_bytes() {
        let mut w = BitWriter::new();
        for i in 0..16 {
            w.bool(i % 3 == 0);
        }
        assert_eq!(w.len_bits(), 16);
        assert_eq!(w.finish().len(), 2);
    }

    #[test]
    fn truncated_packets_fail_cleanly() {
        let mut r = BitReader::new(&[0xFF]);
        assert_eq!(r.u16(), Err(DecodeError::Truncated));
        let mut r = BitReader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(r.varint(), Err(DecodeError::Invalid("varint")));
    }

    #[test]
    fn quantization_error_is_within_half_a_step() {
        for i in 0..1000 {
            let v = -3.0 + i as f32 * 0.006;
            let back = dequantize(quantize(v, -3.0, 3.0, 12), -3.0, 3.0, 12);
            assert!((back - v).abs() <= 6.0 / 4095.0 / 2.0 + 1e-6);
        }
    }
}
