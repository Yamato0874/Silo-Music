//! Шифрование ключей сервисов средствами Windows (DPAPI): расшифровать может только эта учётная запись на этом ПК.
//! В базе хранится как "dpapi:<hex>". На других системах — как есть.

/// Настройки, которые считаются секретами.
pub const SECRET_KEYS: [&str; 7] = [
    "youtube_api_key", "soundcloud_client_id", "soundcloud_client_secret", "jamendo_client_id",
    "google_cse_key", "google_cse_cx", "listenbrainz_token",
];
const PREFIX: &str = "dpapi:";

pub fn is_secret(key: &str) -> bool { SECRET_KEYS.contains(&key) }
pub fn is_protected(v: &str) -> bool { v.starts_with(PREFIX) }

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 { return None; }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Зашифровать; None — если шифрование недоступно (тогда значение хранится как есть).
pub fn protect(plain: &str) -> Option<String> {
    if plain.is_empty() { return Some(String::new()); }
    sys::protect(plain.as_bytes()).map(|b| format!("{PREFIX}{}", hex(&b)))
}
/// Расшифровать значение из базы. Незашифрованное возвращается как есть; None — расшифровать не удалось.
pub fn unprotect(stored: &str) -> Option<String> {
    match stored.strip_prefix(PREFIX) {
        None => Some(stored.to_string()),
        Some(h) => sys::unprotect(&unhex(h)?).and_then(|b| String::from_utf8(b).ok()),
    }
}

#[cfg(windows)]
mod sys {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    fn run(input: &[u8], enc: bool) -> Option<Vec<u8>> {
        let inb = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            if enc { CryptProtectData(&inb, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
            else { CryptUnprotectData(&inb, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
        };
        if ok == 0 || out.pbData.is_null() { return None; }
        let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe { LocalFree(out.pbData as _); }
        Some(v)
    }
    pub fn protect(b: &[u8]) -> Option<Vec<u8>> { run(b, true) }
    pub fn unprotect(b: &[u8]) -> Option<Vec<u8>> { run(b, false) }
}
#[cfg(not(windows))]
mod sys {
    pub fn protect(_: &[u8]) -> Option<Vec<u8>> { None }
    pub fn unprotect(_: &[u8]) -> Option<Vec<u8>> { None }
}

#[cfg(test)]
mod tests {
    #[test]
    fn roundtrip() {
        if let Some(p) = super::protect("AIza-test-key") {
            assert!(super::is_protected(&p));
            assert_eq!(super::unprotect(&p).as_deref(), Some("AIza-test-key"));
        }
        assert_eq!(super::unprotect("plain").as_deref(), Some("plain"));
        assert_eq!(super::unprotect("dpapi:zz"), None);
    }
}
