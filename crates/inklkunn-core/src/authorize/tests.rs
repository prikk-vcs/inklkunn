use super::*;

#[test]
fn m0_denies_everything_for_everyone() {
    let wiki = WikiId::new();
    let principals = [
        Principal::Anonymous,
        Principal::Account("acct".into()),
        Principal::System("import".into()),
    ];
    let resources = [
        Resource::Wiki {
            wiki: wiki.clone(),
            visibility: Visibility::Public,
        },
        Resource::Page {
            wiki: wiki.clone(),
            path: "pages/Home.md".into(),
        },
        Resource::View {
            wiki: wiki.clone(),
            name: "heads/main".into(),
        },
        Resource::Proposal {
            wiki: wiki.clone(),
            id: "p".into(),
        },
    ];
    for p in &principals {
        for a in Action::ALL {
            for r in &resources {
                assert_eq!(
                    authorize(p, a, r),
                    Decision::Deny,
                    "{p:?} {} {r:?}",
                    a.name()
                );
                assert_eq!(r.wiki(), &wiki);
            }
        }
    }
    assert!(Action::Accept.is_consequential());
    assert!(!Action::Read.is_consequential());
}
