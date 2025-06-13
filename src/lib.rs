#![doc = include_str!("../README.md")]
#![no_std]

use core::{
    fmt::{Debug, UpperHex},
    ops::{Bound, RangeBounds},
};

use num::{traits::CheckedShr, PrimInt, Unsigned};

#[inline]
fn bitmask<T: PrimInt + Unsigned>(bit_width: usize) -> T {
    let max_width = core::mem::size_of::<T>() * 8;
    assert!(bit_width <= max_width);
    if bit_width == max_width {
        T::max_value()
    } else {
        T::from((1_usize << bit_width) - 1).unwrap()
    }
}

pub trait BitfieldAccess: AsRef<[u8]> {
    /// Read a bitfield with the given bit indices from a buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use bitfield_access::BitfieldAccess;
    ///
    /// let buffer = [0x12, 0x34, 0x56, 0x78];
    /// assert_eq!(buffer.read_field::<u8>(4..8), 0x2);
    /// assert_eq!(buffer.read_field::<u16>(12..24), 0x456);
    /// assert_eq!(buffer.read_field::<u8>(25..=25), 0x1);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the range of bits is wider than the integer type `T`
    /// or the bit indices are out of bounds.
    #[inline]
    fn read_field<T>(&self, bitrange: impl RangeBounds<usize>) -> T
    where
        T: PrimInt + Unsigned,
    {
        // There's a lot of logic here, but as an inline function the bit range is
        // typically known at compile time, reducing this to just a small handful
        // of shifts and bitwise instructions.
        let data = self.as_ref();
        let start = match bitrange.start_bound() {
            core::ops::Bound::Included(idx) => *idx,
            core::ops::Bound::Excluded(idx) => *idx + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match bitrange.end_bound() {
            core::ops::Bound::Included(idx) => *idx + 1,
            core::ops::Bound::Excluded(idx) => *idx,
            core::ops::Bound::Unbounded => data.len() * 8,
        };

        let storage_width = 8 * core::mem::size_of::<T>();
        let bit_width = end - start;
        assert!(
            bit_width <= storage_width,
            "field width {} exceeds storage width {}",
            bit_width,
            storage_width
        );
        let first_byte = start / 8;
        let last_byte = (end - 1) / 8;
        let num_bytes = last_byte - first_byte + 1;
        let offset = 7 - (end - 1) % 8;
        let mask = bitmask(bit_width);

        // build the result from the last byte (LSB) to the first
        let mut result = T::from(data[last_byte] >> offset).unwrap();
        for i in 1..num_bytes {
            result = result | T::from(data[last_byte - i]).unwrap() << (8 * i - offset);
        }

        result & mask
    }

    /// Write a bitfield with the given bit indices to a buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use bitfield_access::BitfieldAccess;
    ///
    /// let mut buffer = [0x12, 0x34, 0x56, 0x78];
    /// buffer.write_field(4..8, 0xA_u8);
    /// assert_eq!(buffer, [0x1A, 0x34, 0x56, 0x78]);
    /// buffer.write_field(20..=27, 0xBC_u8);
    /// assert_eq!(buffer, [0x1A, 0x34, 0x5B, 0xC8]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the bit indices are out of bounds or the value is too large.
    #[inline]
    fn write_field<T>(&mut self, bitrange: impl RangeBounds<usize>, mut value: T)
    where
        Self: AsMut<[u8]>,
        T: PrimInt + Unsigned + TryInto<u8> + UpperHex + CheckedShr,
        <T as TryInto<u8>>::Error: Debug,
    {
        // There's a lot of logic here, but as an inline function the bit range is
        // typically known at compile time, reducing this to just a small handful
        // of shifts and bitwise instructions.
        let data = self.as_mut();
        let start = match bitrange.start_bound() {
            Bound::Included(idx) => *idx,
            Bound::Excluded(idx) => *idx + 1,
            Bound::Unbounded => 0,
        };
        let mut end = match bitrange.end_bound() {
            Bound::Included(idx) => *idx + 1,
            Bound::Excluded(idx) => *idx,
            Bound::Unbounded => data.len() * 8,
        };
        let first_byte = start / 8;
        let last_byte = (end - 1) / 8;
        let max_value = bitmask(end - start);
        assert!(
            value <= max_value,
            "value {:#X} exceeds maximum field value {:#X}",
            value,
            max_value
        );

        let byte_mask = T::from(0xFF).unwrap();
        let zero = T::from(0x0).unwrap();

        // write in one-byte chunks, from the last (LSB) to the first
        for i in (first_byte..=last_byte).rev() {
            let bit_offset = 7 - (end - 1) % 8;
            let bit_width = core::cmp::min(8 - bit_offset, end - start);
            let bit_mask = bitmask::<u8>(bit_width) << bit_offset;
            let new_bits: u8 = (value & byte_mask).try_into().unwrap();
            data[i] = (data[i] & !bit_mask) | ((new_bits << bit_offset) & bit_mask);
            end -= bit_width;
            value = value.checked_shr(bit_width as u32).unwrap_or(zero);
        }
    }
}

impl<T> BitfieldAccess for T where T: AsRef<[u8]> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[inline(never)]
    fn test_read_field() {
        let buffer = [0x12, 0x34, 0x56, 0x78];

        // Test reading a single byte
        assert_eq!(buffer.read_field::<u8>(4..8), 0x2);
        assert_eq!(buffer.read_field::<u8>(8..16), 0x34);

        // Test reading across byte boundaries
        assert_eq!(buffer.read_field::<u16>(4..20), 0x2345);

        // Test reading the entire buffer
        assert_eq!(buffer.read_field::<u32>(..), 0x12345678);

        // Test reading a single bit
        assert_eq!(buffer.read_field::<u8>(7..8), 0x0);
        assert_eq!(buffer.read_field::<u8>(17..=17), 0x1);
    }

    #[test]
    fn test_write_field() {
        const BUFFER: [u8; 4] = [0x12, 0x34, 0x56, 0x78];

        // Test writing a single byte
        let mut buffer = BUFFER;
        buffer.write_field::<u8>(4..8, 0xA);
        assert_eq!(buffer, [0x1A, 0x34, 0x56, 0x78]);
        buffer.write_field::<u8>(0..8, 0xBC);
        assert_eq!(buffer, [0xBC, 0x34, 0x56, 0x78]);

        // Test writing across byte boundaries
        let mut buffer = BUFFER;
        buffer.write_field::<u8>(12..20, 0xBC);
        assert_eq!(buffer, [0x12, 0x3B, 0xC6, 0x78]);

        // Test writing the entire buffer
        let mut buffer = BUFFER;
        buffer.write_field::<u32>(.., 0x87654321u32);
        assert_eq!(buffer, [0x87, 0x65, 0x43, 0x21]);

        // Test writing a single bit
        let mut buffer = BUFFER;
        buffer.write_field::<u8>(7..8, 0x1);
        assert_eq!(buffer, [0x13, 0x34, 0x56, 0x78]);
        buffer.write_field::<u8>(8..=8, 0x1);
        assert_eq!(buffer, [0x13, 0xB4, 0x56, 0x78]);
        buffer.write_field::<u8>(30..31, 0x1);
        assert_eq!(buffer, [0x13, 0xB4, 0x56, 0x7A]);
    }
}
