//! The host builds' `sheprd-acp`: one archive per release, its signed
//! manifest (schema 5) first, read only through that manifest.

use super::*;

const VERSION: &str = "20261108.3";
const TARGET: &str = "aarch64-unknown-linux-musl";

fn key() -> String {
    hex(SigningKey::from_bytes(&[42; 32]).verifying_key().as_bytes())
}

fn manifest(build: &[u8]) -> Manifest {
    Manifest {
        schema: host_chat::SCHEMA,
        version: VERSION.into(),
        assets: HOST_TARGETS
            .iter()
            .map(|target| Asset {
                target: (*target).into(),
                name: format!("sheprd-acp-{VERSION}-{target}"),
                size: build.len() as u64,
                sha256: hex(&Sha256::digest(build)),
            })
            .collect(),
    }
}

/// The archive the release packs, its manifest signed by `signer` and
/// `build` as every target's program.
fn archive(manifest: &Manifest, signer: u8, build: &[u8]) -> Vec<u8> {
    tampered(manifest, signer, build, None)
}

/// [`archive`], with an unsigned `extra` entry between the manifest and
/// the builds when given.
fn tampered(manifest: &Manifest, signer: u8, build: &[u8], extra: Option<&[u8]>) -> Vec<u8> {
    let bytes = serde_json::to_vec(manifest).unwrap_or_default();
    let signature = SigningKey::from_bytes(&[signer; 32])
        .sign(&bytes)
        .to_bytes();
    let mut packed = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    let mut add = |name: &str, data: &[u8]| {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        packed.append_data(&mut header, name, data).unwrap_or(());
    };
    add("host-chat-manifest.json", &bytes);
    add("host-chat-manifest.sig", &signature);
    if let Some(extra) = extra {
        add("padding", extra);
    }
    for target in HOST_TARGETS {
        add(&format!("{target}/sheprd-acp"), build);
    }
    packed
        .into_inner()
        .and_then(|gz| gz.finish())
        .unwrap_or_default()
}

fn unpack(archive: &[u8], target: &str) -> (Result<()>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let result = host_chat::unpack(
        archive,
        VERSION,
        &key(),
        target,
        &dir.path().join("sheprd-acp"),
        &AtomicBool::new(false),
    );
    (result, dir)
}

#[test]
fn a_build_signed_for_this_release_is_unpacked() -> anyhow::Result<()> {
    let build = b"static sheprd-acp";
    let (result, dir) = unpack(&archive(&manifest(build), 42, build), TARGET);
    result?;
    assert_eq!(std::fs::read(dir.path().join("sheprd-acp"))?, build);
    Ok(())
}

#[test]
fn nothing_is_written_without_the_right_signature_schema_or_target() {
    let build = b"static sheprd-acp";
    let (result, dir) = unpack(&archive(&manifest(build), 7, build), TARGET);
    assert!(matches!(result, Err(Error::Signature(_))));
    assert!(!dir.path().join("sheprd-acp").exists());
    // The host build manifest, signed by the same key, answers for no chat.
    let mut hosts = manifest(build);
    hosts.schema = 4;
    let (result, _) = unpack(&archive(&hosts, 42, build), TARGET);
    assert!(result.is_err());
    let (result, _) = unpack(
        &archive(&manifest(build), 42, build),
        "x86_64-unknown-linux-gnu",
    );
    assert!(matches!(result, Err(Error::UnsupportedTarget)));
}

#[test]
fn a_build_that_differs_from_its_manifest_is_refused() {
    let (result, _) = unpack(
        &archive(&manifest(b"signed bytes"), 42, b"other bytes!"),
        TARGET,
    );
    assert!(matches!(result, Err(Error::ArchiveDigest)));
}

#[test]
fn an_archive_without_its_manifest_first_is_refused() {
    let mut packed = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(3);
    header.set_cksum();
    let _ = packed.append_data(&mut header, format!("{TARGET}/sheprd-acp"), &b"abc"[..]);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = Write::write_all(&mut gz, &packed.into_inner().unwrap_or_default());
    let (result, _) = unpack(&gz.finish().unwrap_or_default(), TARGET);
    assert!(matches!(result, Err(Error::HostChatLayout)));
}

#[test]
fn the_archive_is_named_for_the_release() {
    assert_eq!(
        host_chat::archive_name(VERSION),
        "sheprd-acp-20261108.3-linux-musl.tar.gz"
    );
}

/// Bytes gzip cannot shrink, so the compressed archive is mostly them.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn unsigned_data_past_the_manifest_is_refused_before_it_expands() {
    let build = b"static sheprd-acp";
    // Zeros expand far past what they take compressed.
    let extra = vec![0; 8 * 1024 * 1024];
    let packed = tampered(&manifest(build), 42, build, Some(&extra));
    assert!(packed.len() < 64 * 1024, "small on the wire");
    let (result, dir) = unpack(&packed, TARGET);
    assert!(matches!(result, Err(Error::HostChatExpanded)), "{result:?}");
    assert!(!dir.path().join("sheprd-acp").exists());
    // Builds the manifest signs, however large, still fit.
    let (result, _) = unpack(&archive(&manifest(&extra), 42, &extra), TARGET);
    assert!(result.is_ok(), "{result:?}");
}

/// Reads `bytes`, cancelling once `after` of them were read, and counts
/// what was read after that.
struct CancelAfter<'a> {
    after: usize,
    bytes: &'a [u8],
    cancel: &'a AtomicBool,
    read: usize,
    since: &'a std::cell::Cell<usize>,
}

impl Read for CancelAfter<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = Read::read(&mut &self.bytes[self.read..], buffer)?;
        self.read += count;
        if self.read >= self.after && self.cancel.swap(true, Ordering::Relaxed) {
            self.since.set(self.since.get() + count);
        }
        Ok(count)
    }
}

#[test]
fn cancelling_stops_reading_through_skipped_data() {
    // A signed build large enough that skipping the other target's copy
    // takes many reads.
    let build = noise(4 * 1024 * 1024);
    let packed = archive(&manifest(&build), 42, &build);
    // The last target's build is wanted, so the ones before it are
    // skipped: cancel a quarter of the way in, inside the first.
    let wanted = HOST_TARGETS[HOST_TARGETS.len() - 1];
    assert!(HOST_TARGETS.len() > 1);
    let cancel = AtomicBool::new(false);
    let since = std::cell::Cell::new(0);
    let reader = CancelAfter {
        after: packed.len() / 4,
        bytes: &packed,
        cancel: &cancel,
        read: 0,
        since: &since,
    };
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
    let destination = dir.path().join("sheprd-acp");
    let result = host_chat::unpack(reader, VERSION, &key(), wanted, &destination, &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(!destination.exists(), "nothing written");
    assert!(
        since.get() < 256 * 1024,
        "read {} bytes after cancelling",
        since.get()
    );
}
