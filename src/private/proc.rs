use crate::private::{PopBits, PushBits};
use crate::Specifier;

#[doc(hidden)]
#[inline(always)]
pub fn read_specifier<T>(bytes: &[u8], offset: usize) -> <T as Specifier>::Base
where
    T: Specifier,
{
    let end = offset + <T as Specifier>::BITS;
    let ls_byte = offset / 8; // compile-time
    let ms_byte = (end - 1) / 8; // compile-time
    let lsb_offset = offset % 8; // compile-time
    let msb_offset = end % 8; // compile-time
    let msb_offset = if msb_offset == 0 { 8 } else { msb_offset };

    let mut buffer = <<T as Specifier>::Base as Default>::default();

    if lsb_offset == 0 && msb_offset == 8 {
        // Edge-case for whole bytes manipulation.
        for byte in bytes[ls_byte..(ms_byte + 1)].iter().rev() {
            buffer.push_bits(8, *byte)
        }
    } else {
        if ls_byte != ms_byte {
            // Most-significant byte
            buffer.push_bits(msb_offset as u32, bytes[ms_byte]);
        }
        if ms_byte - ls_byte >= 2 {
            // Middle bytes
            for byte in bytes[(ls_byte + 1)..ms_byte].iter().rev() {
                buffer.push_bits(8, *byte);
            }
        }
        if ls_byte == ms_byte {
            buffer.push_bits(<T as Specifier>::BITS as u32, bytes[ls_byte] >> lsb_offset);
        } else {
            buffer.push_bits(8 - lsb_offset as u32, bytes[ls_byte] >> lsb_offset);
        }
    }
    buffer
}

#[doc(hidden)]
#[inline(always)]
pub fn write_specifier<T>(bytes: &mut [u8], offset: usize, new_val: <T as Specifier>::Base)
where
    T: Specifier,
{
    let end = offset + <T as Specifier>::BITS;
    let ls_byte = offset / 8; // compile-time
    let ms_byte = (end - 1) / 8; // compile-time
    let lsb_offset = offset % 8; // compile-time
    let msb_offset = end % 8; // compile-time
    let msb_offset = if msb_offset == 0 { 8 } else { msb_offset };

    let mut input = new_val;

    if lsb_offset == 0 && msb_offset == 8 {
        // Edge-case for whole bytes manipulation.
        for byte in bytes[ls_byte..(ms_byte + 1)].iter_mut() {
            *byte = input.pop_bits(8);
        }
    } else {
        // Least-significant byte
        let stays_same = bytes[ls_byte]
            & (if ls_byte == ms_byte && msb_offset != 8 {
                !((0x01 << msb_offset) - 1)
            } else {
                0u8
            } | ((0x01 << lsb_offset as u32) - 1));
        let overwrite = input.pop_bits(8 - lsb_offset as u32);
        bytes[ls_byte] = stays_same | (overwrite << lsb_offset as u32);
        if ms_byte - ls_byte >= 2 {
            // Middle bytes
            for byte in bytes[(ls_byte + 1)..ms_byte].iter_mut() {
                *byte = input.pop_bits(8);
            }
        }
        if ls_byte != ms_byte {
            // Most-significant byte
            if msb_offset == 8 {
                // We don't need to respect what was formerly stored in the byte.
                bytes[ms_byte] = input.pop_bits(msb_offset as u32);
            } else {
                // All bits that do not belong to this field should be preserved.
                let stays_same = bytes[ms_byte] & !((0x01 << msb_offset) - 1);
                let overwrite = input.pop_bits(msb_offset as u32);
                bytes[ms_byte] = stays_same | overwrite;
            }
        }
    }
}