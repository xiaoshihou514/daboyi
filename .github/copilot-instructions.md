# Copilot Instructions for daboyi

## Rust coding rules

- **No `unsafe` blocks.** All code must be safe Rust. No `unsafe fn`, no `unsafe impl`, no `unsafe {}`.

## File deletion

- **Never use `rm`.** To delete files, always use: `kioclient move "file://$the_file_to_delete" 'trash:/'`
