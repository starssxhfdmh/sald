// JIT Runtime Helpers - Called from JIT-compiled code

/// Tag bits for JIT value representation (NaN-boxing)
/// 
/// Layout (64-bit):
/// - Number: raw f64 bits (except NaN range)
/// - Null:   0x7FF8_0000_0000_0000 | 0
/// - True:   0x7FF8_0000_0000_0000 | 1
/// - False:  0x7FF8_0000_0000_0000 | 2
/// - Object: 0x7FFC_0000_0000_0000 | pointer (48-bit)
pub const TAG_MASK: u64 = 0xFFFF_0000_0000_0000;
pub const TAG_NAN: u64 = 0x7FF8_0000_0000_0000;
pub const TAG_NULL: u64 = TAG_NAN | 0;
pub const TAG_TRUE: u64 = TAG_NAN | 1;
pub const TAG_FALSE: u64 = TAG_NAN | 2;
pub const TAG_OBJECT: u64 = 0x7FFC_0000_0000_0000;
pub const PTR_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Check if a tagged value is a number
#[inline(always)]
pub fn is_number(v: u64) -> bool {
    (v & TAG_MASK) != TAG_NAN
}

/// Check if a tagged value is null
#[inline(always)]
pub fn is_null(v: u64) -> bool {
    v == TAG_NULL
}

/// Check if a tagged value is truthy
#[inline(always)]
pub fn is_truthy(v: u64) -> bool {
    v != TAG_NULL && v != TAG_FALSE
}

/// Encode a number as tagged value
#[inline(always)]
pub fn encode_number(n: f64) -> u64 {
    n.to_bits()
}

/// Decode a tagged value to number
#[inline(always)]
pub fn decode_number(v: u64) -> f64 {
    f64::from_bits(v)
}

/// Encode a boolean as tagged value
#[inline(always)]
pub fn encode_bool(b: bool) -> u64 {
    if b { TAG_TRUE } else { TAG_FALSE }
}

/// Runtime helper: print value (for debugging)
#[no_mangle]
pub extern "C" fn jit_runtime_print(value: u64) {
    if is_number(value) {
        println!("{}", decode_number(value));
    } else if value == TAG_NULL {
        println!("null");
    } else if value == TAG_TRUE {
        println!("true");
    } else if value == TAG_FALSE {
        println!("false");
    } else {
        println!("<object>");
    }
}

/// Runtime helper: create error
#[no_mangle]
pub extern "C" fn jit_runtime_error(msg_ptr: *const u8, msg_len: usize) -> ! {
    let msg = unsafe {
        let slice = std::slice::from_raw_parts(msg_ptr, msg_len);
        std::str::from_utf8_unchecked(slice)
    };
    panic!("JIT runtime error: {}", msg);
}

/// Runtime helper: division by zero check
#[no_mangle]
pub extern "C" fn jit_check_divide_by_zero(divisor: f64) {
    if divisor == 0.0 {
        panic!("Division by zero");
    }
}
