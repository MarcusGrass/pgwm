pub mod bat;
pub mod cpu;
pub mod mem;
pub mod net;

#[inline]
fn find_in_haystack(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    let hay_len = haystack.len();
    let needle_len = needle.len();
    (0..hay_len - needle_len).find(|&i| &haystack[i..i + needle_len] == needle)
}

#[inline]
fn find_byte(tgt: u8, bytes: &[u8]) -> Option<usize> {
    for (ind, byte) in bytes.iter().enumerate() {
        if byte == &tgt {
            return Some(ind);
        }
    }
    None
}
