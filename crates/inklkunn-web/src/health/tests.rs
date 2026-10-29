use super::*;

#[test]
fn every_list_has_its_address_and_the_address_leads_back_to_it() {
    let slugs: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
    assert_eq!(
        slugs,
        [
            "wanted",
            "orphans",
            "dead-ends",
            "redirects",
            "wants-help",
            "stale"
        ]
    );
    for kind in Kind::ALL {
        assert_eq!(Kind::from_slug(kind.slug()), Some(kind));
    }
    assert_eq!(Kind::from_slug("Wanted"), None);
    assert_eq!(Kind::from_slug(""), None);
    assert_eq!(Kind::from_slug("../wanted"), None);
}
