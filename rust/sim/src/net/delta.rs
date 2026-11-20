//! Field-by-field delta compression for any encoder written against
//! [`BitSink`] and [`BitSource`].
//!
//! The encoder runs once against a [`Recorder`] on the baseline (the value
//! the receiver already has) to list its fields, then again against a
//! [`DeltaWriter`] on the new value. Each field is compared with the
//! baseline's field at the same position: an unchanged field costs one bit,
//! a changed 32-bit field (a float, a counter) is sent as its XOR with the
//! baseline's, leading zeros dropped, and anything else is sent raw. The
//! decoder runs the matching reader against a [`DeltaReader`] over the same
//! baseline. Nothing here knows what the fields mean, so a field added to an
//! encoder is delta-compressed with no further work, and the result is
//! bit-exact.
//!
//! This is Quake III Arena's model (`MSG_WriteDeltaPlayerstate` and
//! `MSG_WriteDeltaEntity` in `qcommon/msg.c`, GPL; ideas only, no code
//! copied): one "changed" bit per field against the last state the client
//! acknowledged, the field only when it changed. Quake lists its fields in a
//! table; here the encoder's own call order is the table. The XOR of a float
//! with its previous value, leading zeros dropped, is the float compression
//! of Facebook's Gorilla (Pelkonen et al., VLDB 2015): a timer or a velocity
//! that moved a little keeps its sign, exponent, and top mantissa bits.
//!
//! When a variable-length part (an enum's payload) changes shape, later
//! fields line up against different baseline fields. Where the widths differ
//! the field goes raw with no flag (both sides know both widths); where they
//! match it is compared anyway. Either way the result decodes exactly; it is
//! only a little larger until the baseline catches up.

use super::bits::{BitReader, BitSink, BitSource, BitWriter, DecodeError, DecodeResult};

/// One recorded field: its value and width in bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub value: u32,
    pub count: u8,
}

/// Lists the fields an encoder writes, in order.
#[derive(Default)]
pub struct Recorder {
    pub fields: Vec<Field>,
}

impl BitSink for Recorder {
    fn bits(&mut self, value: u32, count: u32) {
        self.fields.push(Field {
            value,
            count: count as u8,
        });
    }
}

/// The fields `encode` writes.
pub fn record(encode: impl FnOnce(&mut Recorder)) -> Vec<Field> {
    let mut recorder = Recorder::default();
    encode(&mut recorder);
    recorder.fields
}

/// Fields this narrow go raw: a flag would cost as much as the field.
const RAW_BITS: u32 = 2;
/// Bits for the leading-zero count of a changed 32-bit field's XOR.
const ZEROS_BITS: u32 = 5;

/// How field number `index` of width `count` is coded against `base`.
enum Coding {
    /// No comparable baseline field: the value, raw.
    Raw,
    /// A "changed" flag, then the value raw if it changed.
    Flagged(u32),
    /// A "changed" flag, then the XOR with the baseline, leading zeros dropped.
    Xor(u32),
}

fn coding(base: &[Field], index: usize, count: u32) -> Coding {
    match base.get(index) {
        Some(f) if u32::from(f.count) == count && count > RAW_BITS => {
            if count == 32 {
                Coding::Xor(f.value)
            } else {
                Coding::Flagged(f.value)
            }
        }
        _ => Coding::Raw,
    }
}

/// Writes fields to `out` as changes from `base` (see the module docs).
pub struct DeltaWriter<'a> {
    out: &'a mut BitWriter,
    base: &'a [Field],
    index: usize,
}

impl<'a> DeltaWriter<'a> {
    pub fn new(out: &'a mut BitWriter, base: &'a [Field]) -> Self {
        Self {
            out,
            base,
            index: 0,
        }
    }
}

impl BitSink for DeltaWriter<'_> {
    fn bits(&mut self, value: u32, count: u32) {
        let coding = coding(self.base, self.index, count);
        self.index += 1;
        match coding {
            Coding::Raw => self.out.bits(value, count),
            Coding::Flagged(base) => {
                self.out.bool(value != base);
                if value != base {
                    self.out.bits(value, count);
                }
            }
            Coding::Xor(base) => {
                let xor = value ^ base;
                self.out.bool(xor != 0);
                if xor != 0 {
                    // Not zero, so at most 31 leading zeros.
                    let zeros = xor.leading_zeros();
                    self.out.bits(zeros, ZEROS_BITS);
                    self.out.bits(xor, 32 - zeros);
                }
            }
        }
    }
}

/// Reads fields written by [`DeltaWriter`] against the same `base`.
pub struct DeltaReader<'r, 'a> {
    input: &'r mut BitReader<'a>,
    base: &'r [Field],
    index: usize,
}

impl<'r, 'a> DeltaReader<'r, 'a> {
    pub fn new(input: &'r mut BitReader<'a>, base: &'r [Field]) -> Self {
        Self {
            input,
            base,
            index: 0,
        }
    }
}

impl BitSource for DeltaReader<'_, '_> {
    fn bits(&mut self, count: u32) -> DecodeResult<u32> {
        let coding = coding(self.base, self.index, count);
        self.index += 1;
        match coding {
            Coding::Raw => self.input.bits(count),
            Coding::Flagged(base) => {
                if self.input.bool()? {
                    self.input.bits(count)
                } else {
                    Ok(base)
                }
            }
            Coding::Xor(base) => {
                if !self.input.bool()? {
                    return Ok(base);
                }
                let zeros = self.input.bits(ZEROS_BITS)?;
                let xor = self.input.bits(32 - zeros)?;
                // A real XOR has exactly `zeros` leading zeros.
                if xor.leading_zeros() != zeros {
                    return Err(DecodeError::Invalid("delta field"));
                }
                Ok(base ^ xor)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(w: &mut impl BitSink, v: &(f32, u16, bool, Option<f32>, u32)) {
        w.f32(v.0);
        w.u16(v.1);
        w.bool(v.2);
        w.bool(v.3.is_some());
        if let Some(x) = v.3 {
            w.f32(x);
        }
        w.u32(v.4);
    }

    fn read(r: &mut impl BitSource) -> DecodeResult<(f32, u16, bool, Option<f32>, u32)> {
        Ok((
            r.f32()?,
            r.u16()?,
            r.bool()?,
            if r.bool()? { Some(r.f32()?) } else { None },
            r.u32()?,
        ))
    }

    fn round_trip(
        base: &(f32, u16, bool, Option<f32>, u32),
        value: &(f32, u16, bool, Option<f32>, u32),
    ) -> usize {
        let fields = record(|r| write(r, base));
        let mut out = BitWriter::new();
        write(&mut DeltaWriter::new(&mut out, &fields), value);
        let bits = out.len_bits();
        let bytes = out.finish();
        let mut input = BitReader::new(&bytes);
        let decoded = read(&mut DeltaReader::new(&mut input, &fields)).unwrap();
        assert_eq!(decoded.0.to_bits(), value.0.to_bits());
        assert_eq!(&decoded, value);
        bits
    }

    #[test]
    fn unchanged_fields_cost_a_bit() {
        let v = (1.5, 300, true, Some(-2.0), 77);
        // Five flags and the two bare booleans.
        assert_eq!(round_trip(&v, &v), 6);
    }

    #[test]
    fn small_float_changes_cost_less_than_the_float() {
        let base = (0.7, 300, false, None, 1000);
        let value = (0.7 - 1.0 / 60.0, 300, false, None, 1001);
        let bits = round_trip(&base, &value);
        assert!(bits < 32 + 6, "{bits} bits");
    }

    #[test]
    fn a_shape_change_still_decodes_exactly() {
        let a = (0.0, 1, false, None, 5);
        let b = (-0.0, 2, true, Some(f32::MIN_POSITIVE), u32::MAX);
        round_trip(&a, &b);
        round_trip(&b, &a);
        round_trip(&b, &(f32::INFINITY, 0, false, Some(1e30), 0));
    }

    #[test]
    fn garbage_fails_cleanly() {
        let fields = record(|r| write(r, &(1.0, 2, true, Some(3.0), 4)));
        for seed in 0u32..2000 {
            let bytes: Vec<u8> = (0..12)
                .map(|i| (seed.wrapping_mul(2_654_435_761).rotate_left(i * 5) >> 7) as u8)
                .collect();
            let mut input = BitReader::new(&bytes);
            let _ = read(&mut DeltaReader::new(&mut input, &fields));
        }
    }
}
