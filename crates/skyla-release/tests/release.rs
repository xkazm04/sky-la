//! WP-33 acceptance (release tooling): the versions agree, the bundles are
//! collected with checksums and a manifest the app's update check accepts,
//! and a signature made with the release key verifies.

#![allow(clippy::unwrap_used)]

use std::path::Path;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn the_tag_crates_shell_and_changelog_agree() {
    let v = skyla_release::workspace_version(root()).unwrap();
    assert_eq!(skyla_release::shell_version(root()).unwrap(), v);
    assert_eq!(
        skyla_release::check_version(root(), &format!("v{v}")).unwrap(),
        v
    );
    assert!(skyla_release::check_version(root(), "v99.0.0").is_err());
    assert!(!skyla_release::notes(root(), &v).unwrap().is_empty());
    assert!(skyla_release::notes(root(), "99.0.0").is_err());
}

#[test]
fn bundles_are_collected_summed_and_signed() {
    let v = skyla_release::workspace_version(root()).unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    let nested = artifacts.path().join("linux/bundle/deb");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join(format!("sky-la_{v}_amd64.deb")), b"deb bytes").unwrap();
    std::fs::create_dir_all(artifacts.path().join("mac/sky-la.app/Contents")).unwrap();
    std::fs::write(
        artifacts.path().join("mac/sky-la.app/Contents/x.dmg"),
        b"inside an app: skipped",
    )
    .unwrap();
    std::fs::write(
        artifacts.path().join(format!("sky-la_{v}_x64-setup.exe")),
        b"exe",
    )
    .unwrap();
    std::fs::write(artifacts.path().join("notes.txt"), b"not a bundle").unwrap();

    let out = tempfile::tempdir().unwrap();
    let files =
        skyla_release::collect(root(), artifacts.path(), out.path(), &v, "2026-11-02").unwrap();
    assert_eq!(
        files,
        [
            format!("sky-la_{v}_amd64.deb"),
            format!("sky-la_{v}_x64-setup.exe"),
            "SHA256SUMS".into(),
            "latest.json".into()
        ]
    );
    assert_eq!(skyla_release::verify_sums(out.path()).unwrap(), 2);
    std::fs::write(out.path().join(format!("sky-la_{v}_amd64.deb")), b"swapped").unwrap();
    assert!(skyla_release::verify_sums(out.path()).is_err());

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.path().join("latest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["version"], v.as_str());
    assert_eq!(
        manifest["page"],
        format!("https://github.com/xkazm04/sky-la/releases/tag/v{v}").as_str()
    );

    // Signed with a password-protected key, verified with its public half.
    let kp = minisign::KeyPair::generate_encrypted_keypair(Some("pw".into())).unwrap();
    let secret = kp.sk.to_box(None).unwrap().into_string();
    let sig =
        skyla_release::sign(&out.path().join("latest.json"), &secret, Some("pw".into())).unwrap();
    let pk = minisign_verify::PublicKey::from_base64(&kp.pk.to_base64()).unwrap();
    let signature =
        minisign_verify::Signature::decode(&std::fs::read_to_string(sig).unwrap()).unwrap();
    let bytes = std::fs::read(out.path().join("latest.json")).unwrap();
    assert!(pk.verify(&bytes, &signature, false).is_ok());
    assert!(
        skyla_release::sign(
            &out.path().join("latest.json"),
            &secret,
            Some("wrong".into())
        )
        .is_err()
    );
}
