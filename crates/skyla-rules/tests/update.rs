//! WP-20 acceptance: a pack update is installed only when a trusted key
//! signed exactly these bytes, and only when it's a newer valid pack.

use std::io::Cursor;

use skyla_rules::{Pack, UpdateError, verify_pack_update};

const PACK: &str = include_str!("../../../rules/cz/2026/pack.toml");

fn keys() -> (minisign::PublicKey, minisign::SecretKey) {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("keypair");
    (kp.pk, kp.sk)
}

fn sign(text: &str, pk: &minisign::PublicKey, sk: &minisign::SecretKey) -> String {
    minisign::sign(
        Some(pk),
        sk,
        Cursor::new(text.as_bytes()),
        Some("sky-la rule pack"),
        None,
    )
    .expect("sign")
    .to_string()
}

fn bumped() -> String {
    PACK.replacen("version = \"2026.1\"", "version = \"2026.2\"", 1)
}

#[test]
fn a_signed_newer_pack_is_accepted() {
    let (pk, sk) = keys();
    let current = Pack::cz_2026().expect("pack");
    let update = bumped();
    let sig = sign(&update, &pk, &sk);
    let key = pk.to_base64();
    let pack = verify_pack_update(&update, &sig, &[key.as_str()], &current).expect("accepted");
    assert_eq!(pack.provenance(), "cz-2026@2026.2");
}

#[test]
fn a_tampered_or_foreign_signed_pack_is_refused() {
    let (pk, sk) = keys();
    let current = Pack::cz_2026().expect("pack");
    let update = bumped();
    let sig = sign(&update, &pk, &sk);
    let key = pk.to_base64();

    // One value changed after signing: the standard VAT rate.
    let tampered = update.replacen("value = \"21\"", "value = \"20\"", 1);
    assert_ne!(tampered, update);
    let err = verify_pack_update(&tampered, &sig, &[key.as_str()], &current).expect_err("tampered");
    assert!(matches!(err, UpdateError::Tampered), "{err}");
    assert!(err.to_string().contains("nothing was changed"));

    // Signed by a key the app doesn't trust.
    let (other_pk, other_sk) = keys();
    let foreign = sign(&update, &other_pk, &other_sk);
    assert!(matches!(
        verify_pack_update(&update, &foreign, &[key.as_str()], &current),
        Err(UpdateError::Tampered)
    ));

    // A trusted key among several is enough.
    let other = other_pk.to_base64();
    assert!(
        verify_pack_update(&update, &foreign, &[key.as_str(), other.as_str()], &current).is_ok()
    );

    assert!(matches!(
        verify_pack_update(&update, &sig, &[], &current),
        Err(UpdateError::NoTrustedKeys)
    ));
    assert!(matches!(
        verify_pack_update(&update, "not a signature", &[key.as_str()], &current),
        Err(UpdateError::BadSignature(_))
    ));
}

#[test]
fn a_signed_pack_must_still_be_valid_newer_and_the_same_pack() {
    let (pk, sk) = keys();
    let key = pk.to_base64();
    let current = Pack::cz_2026().expect("pack");

    let same = sign(PACK, &pk, &sk);
    assert!(matches!(
        verify_pack_update(PACK, &same, &[key.as_str()], &current),
        Err(UpdateError::NotNewer { .. })
    ));

    let broken = bumped().replacen("cite = { act = \"zdph\"", "cite = { act = \"nowhere\"", 1);
    let sig = sign(&broken, &pk, &sk);
    assert!(matches!(
        verify_pack_update(&broken, &sig, &[key.as_str()], &current),
        Err(UpdateError::Invalid(_))
    ));

    let other = bumped().replacen("id = \"cz-2026\"", "id = \"cz-2027\"", 1);
    let sig = sign(&other, &pk, &sk);
    assert!(matches!(
        verify_pack_update(&other, &sig, &[key.as_str()], &current),
        Err(UpdateError::OtherPack { .. })
    ));
}
