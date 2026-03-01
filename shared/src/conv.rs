//! Safe numeric conversion helpers.
//!
//! Centralises the small number of lossy numeric casts (float ↔ integer) that
//! have no `From`/`Into` impl in std.  All `as` casts in the project are
//! confined to this module; business logic calls these functions instead.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

/// `f64 → u64`, clamped to `[0, u64::MAX]`.
#[inline]
pub fn f64_to_u64(v: f64) -> u64 {
    if v <= 0.0 {
        0
    } else {
        // Rust 1.45+: float-to-int `as` is saturating — no UB.
        v as u64
    }
}

/// `f64 → u32`, clamped to `[0, u32::MAX]`.
#[inline]
pub fn f64_to_u32(v: f64) -> u32 {
    if v <= 0.0 {
        0
    } else if v >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        v as u32
    }
}

/// `f32 → i64`, truncating toward zero.
#[inline]
pub fn f32_to_i64(v: f32) -> i64 {
    v as i64
}

/// `u32 → f32` (may lose precision for values > 2^24).
#[inline]
pub fn u32_to_f32(v: u32) -> f32 {
    v as f32
}

/// `u32 → f64` (lossless).
#[inline]
pub fn u32_to_f64(v: u32) -> f64 {
    f64::from(v)
}

/// `usize → f32` (may lose precision for large values).
#[inline]
pub fn usize_to_f32(v: usize) -> f32 {
    v as f32
}

/// `usize → u32`, saturating on overflow.
#[inline]
pub fn usize_to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// `f64 → f32` (may lose precision).
#[inline]
pub fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

/// `u64 → f64` (may lose precision for values > 2^53).
#[inline]
pub fn u64_to_f64(v: u64) -> f64 {
    v as f64
}

/// `f32 → u32`, saturating (negative → 0, > u32::MAX → u32::MAX).
#[inline]
pub fn f32_to_u32(v: f32) -> u32 {
    v.clamp(0.0, u32::MAX as f32) as u32
}

/// `u32 → usize` (infallible on 32/64-bit platforms).
#[inline]
pub fn u32_to_usize(v: u32) -> usize {
    // Infallible on any platform where usize >= 32 bits.
    usize::try_from(v).unwrap()
}
