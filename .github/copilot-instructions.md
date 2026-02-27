# Copilot Instructions for daboyi

## Rust coding rules

- **No `as` casts.** All numeric conversions must use named helper functions from `shared/src/conv.rs` (e.g. `f64_to_f32`, `u32_to_usize`, `usize_to_u32`). If a conversion you need doesn't exist yet, add it to `conv.rs` using `From`/`TryFrom`/`.try_into().unwrap()` — never raw `as`.
- **No `unsafe` blocks.** All code must be safe Rust. No `unsafe fn`, no `unsafe impl`, no `unsafe {}`.

## File deletion

- **Never use `rm`.** To delete files, always use: `kioclient move "file://$the_file_to_delete" 'trash:/'`