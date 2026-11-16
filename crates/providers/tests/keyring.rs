//! The real Windows Credential Manager. Ignored: it writes (and removes) a
//! credential "Recap-test:roundtrip". `cargo test -p recap-providers --test keyring -- --ignored`

#![cfg(windows)]

use recap_providers::{Keyring, Secrets};

#[test]
#[ignore]
fn a_key_round_trips_through_credential_manager() {
    let store = Keyring::with_service("Recap-test");
    store.delete("roundtrip").unwrap();
    assert_eq!(store.get("roundtrip").unwrap(), None);
    store.set("roundtrip", "sk_not_a_real_key_1234").unwrap();
    assert_eq!(store.get("roundtrip").unwrap().as_deref(), Some("sk_not_a_real_key_1234"));
    store.delete("roundtrip").unwrap();
    assert_eq!(store.get("roundtrip").unwrap(), None);
}
