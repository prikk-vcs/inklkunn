//! Claim and source files for the page tests (RFC 005 part 2 D-1, D-2), written with the core's own types, so a
//! test says what a claim is and the file is the one the wiki reads. Ids are invented, and so is every sentence.

use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Digest, Expression, Lifecycle, Link, Source,
    SourceId, SourceKind, SourceLink, Standing, StatementKind, block_digest, claim_path,
    source_path,
};

use super::{World, page};

/// The frontmatter `id` of the page the claims are said in (`Home`).
pub const HOME_ID: &str = "page-home";

/// A claim id with a distinguishing tail (a valid version-7 UUID).
pub fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

/// A source id with a distinguishing tail.
pub fn source_id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

/// The digest of a block as a page has it: its text, the `^id` marker included.
pub fn digest_of(block: &str) -> Digest {
    let found = inklkunn_markup::blocks(block);
    block_digest(found.first().expect("a block"))
}

/// `Home`, whose first paragraph is `first` (the block `^b1`) and whose second says nothing.
pub fn home_with(first: &str) -> String {
    page(
        &["title: Home", &format!("id: {HOME_ID}")],
        &format!("{first} ^b1\n\nPlain words, no claim.\n"),
    )
}

/// The block `Home`'s first paragraph is, as the page has it.
pub fn block_of(first: &str) -> String {
    format!("{first} ^b1")
}

/// A claim said in `Home`'s first paragraph, an observation, with its status judged on `judged` (the paragraph
/// as it was) and no source yet.
pub fn claim(n: u8, standing: Standing, proposal: Option<&str>, judged: &str) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new(HOME_ID, "b1").expect("an expression")];
    c.community_status = CommunityStatus {
        status: standing,
        proposal: proposal.map(str::to_owned),
        judged_on: (standing != Standing::Unknown).then(|| digest_of(&block_of(judged))),
    };
    c
}

/// A claim's relation to a source, judged on `judged`.
pub fn link(source: u8, relation: Link, bearing: Bearing, judged: &str) -> SourceLink {
    SourceLink {
        source: source_id(source),
        relation,
        bearing,
        note: None,
        judged_on: digest_of(&block_of(judged)),
    }
}

/// A source of this kind.
pub fn source(n: u8, kind: SourceKind) -> Source {
    let mut s = Source::new(source_id(n));
    s.kind = kind;
    s
}

/// The files of these claims and sources, with their paths.
pub fn files(claims: &[Claim], sources: &[Source]) -> Vec<(String, String)> {
    claims
        .iter()
        .map(|c| (claim_path(&c.id), c.to_yaml()))
        .chain(sources.iter().map(|s| (source_path(&s.id), s.to_yaml())))
        .collect()
}

/// Write these claims and sources into the wiki in one change, with `Home` as `home`.
pub fn put(w: &World, home: &str, claims: &[Claim], sources: &[Source]) {
    let mut all = files(claims, sources);
    all.push(("pages/Home.md".to_owned(), home.to_owned()));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    w.fake.set_files(&borrowed);
}
