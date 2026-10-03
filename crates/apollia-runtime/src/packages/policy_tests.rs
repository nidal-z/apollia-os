//! The `[packages]` section of a loaded organisation policy: a signature
//! required, and the publishers trusted.

use apollia_permissions::{parse_policy, PackagesPolicy};
use apollia_tools::apo_archive::SigningKeyPair;
use apollia_tools::TrustedKeyOrigin;

use super::tests::{archive, manifest, Env};
use super::*;
use crate::audit_journal::AuditJournalHandle;
use crate::governance::PackagePolicyGovernance;

const KEY: &[u8] = b"package-policy-tests-journal-0001";

/// The `[packages]` section of a policy document carrying `packages`.
fn packages_section(packages: &str) -> PackagesPolicy {
    parse_policy(&format!(
        "schema_version = 1\nsigned_at = 2026-10-02T07:30:00Z\n[packages]\n{packages}"
    ))
    .unwrap()
    .packages
}

/// What a preview or an installation reads, under the governance and the
/// trusted publishers of `packages`.
struct PolicyEnv {
    env: Env,
    governance: PackagePolicyGovernance,
    keys: apollia_tools::TrustedKeyRepository,
}

impl PolicyEnv {
    async fn new(packages: &PackagesPolicy) -> Self {
        let env = Env::new();
        let journal = AuditJournalHandle::open_with_key_bytes(
            &env.data_dir().join("audit_journal.db"),
            KEY.to_vec(),
        )
        .await
        .unwrap();
        let keys = apollia_tools::TrustedKeyRepository::open(&env.data_dir().join("agents.db"))
            .unwrap()
            .with_policy_publishers(&packages.trusted_publishers);
        Self {
            governance: PackagePolicyGovernance::new(journal, packages),
            keys,
            env,
        }
    }

    fn sources(&self) -> PreviewSources<'_> {
        PreviewSources {
            governance: &self.governance,
            trusted_keys: &self.keys,
            ..self.env.sources()
        }
    }

    fn signed(&self, file: &str, key: Option<&SigningKeyPair>) -> std::path::PathBuf {
        archive(self.env.data_dir(), file, &manifest("1.0.0", &[], ""), key)
    }

    fn install(&self, apo: &std::path::Path) -> Result<StagedInstall, PackageError> {
        let options = InstallOptions {
            actor: "test".into(),
            ..InstallOptions::default()
        };
        prepare(apo, &options, &self.sources(), self.env.data_dir())
    }
}

#[tokio::test]
async fn test_require_signature_refuses_an_unsigned_package() {
    // GIVEN a loaded policy requiring a signature, and an unsigned package
    let env = PolicyEnv::new(&packages_section("require_signature = true\n")).await;
    let apo = env.signed("u.apo", None);

    // WHEN it is previewed, then installed
    let preview = preview(&apo, &env.sources()).unwrap();
    let installed = env.install(&apo);

    // THEN the signature question answers yes and the installation is refused
    assert!(preview.signature_required);
    assert_eq!(preview.refusal, Some(InstallRefusal::SignatureRequired));
    assert!(matches!(
        installed,
        Err(PackageError::Refused(InstallRefusal::SignatureRequired))
    ));
}

#[tokio::test]
async fn test_trusted_publishers_refuse_a_package_signed_by_another_key() {
    // GIVEN a policy requiring a signature and trusting one publisher, a key
    //   the user imported that the policy does not name, and a package signed
    //   by each
    let trusted = SigningKeyPair::from_seed(&[11u8; 32]);
    let other = SigningKeyPair::from_seed(&[12u8; 32]);
    let env = PolicyEnv::new(&packages_section(&format!(
        "require_signature = true\ntrusted_publishers = [\"{}\"]\n",
        trusted.public_key().fingerprint()
    )))
    .await;
    env.keys
        .import(&other.public_key().to_hex(), "Mine", TrustedKeyOrigin::User)
        .unwrap();
    let by_trusted = env.signed("t.apo", Some(&trusted));
    let by_other = env.signed("o.apo", Some(&other));

    // WHEN each is previewed and installed
    let trusted_preview = preview(&by_trusted, &env.sources()).unwrap();
    let other_preview = preview(&by_other, &env.sources()).unwrap();
    let other_install = env.install(&by_other);

    // THEN the publisher the policy trusts installs, shown trusted under the
    //   name it declares, and the other is refused naming its key
    assert_eq!(trusted_preview.refusal, None);
    assert!(trusted_preview.signature.trusted);
    assert_eq!(trusted_preview.signature.publisher, "Acme");
    assert!(env.install(&by_trusted).is_ok());
    let refused = InstallRefusal::PublisherNotTrusted {
        fingerprint: other.public_key().fingerprint(),
    };
    assert!(!other_preview.signature.trusted);
    assert_eq!(other_preview.refusal, Some(refused.clone()));
    assert!(matches!(other_install, Err(PackageError::Refused(r)) if r == refused));
}

#[tokio::test]
async fn test_trusted_publishers_without_require_signature_only_inform() {
    // GIVEN a policy trusting one publisher without requiring a signature, and
    //   a package signed by another key
    let trusted = SigningKeyPair::from_seed(&[13u8; 32]);
    let other = SigningKeyPair::from_seed(&[14u8; 32]);
    let env = PolicyEnv::new(&packages_section(&format!(
        "trusted_publishers = [\"{}\"]\n",
        trusted.public_key().fingerprint()
    )))
    .await;
    let apo = env.signed("o.apo", Some(&other));

    // WHEN it is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN it is not refused, and the publisher reads as not verified
    assert_eq!(preview.refusal, None);
    assert!(preview
        .notices
        .contains(&PackageNotice::UntrustedPublisher {
            fingerprint: other.public_key().fingerprint(),
        }));
}

#[tokio::test]
async fn test_without_a_policy_an_unsigned_package_installs_with_a_notice() {
    // GIVEN no policy: the empty [packages] section an absent policy reads as
    let absent = apollia_permissions::PolicyVerdict::Absent.packages();
    let env = PolicyEnv::new(&absent).await;
    let apo = env.signed("u.apo", None);

    // WHEN it is previewed, then installed
    let preview = preview(&apo, &env.sources()).unwrap();
    let installed = env.install(&apo);

    // THEN nothing requires a signature, the package carries the unsigned
    //   notice, and it installs
    assert!(!preview.signature_required);
    assert_eq!(preview.refusal, None);
    assert!(preview.notices.contains(&PackageNotice::Unsigned));
    assert!(installed.is_ok());
}
