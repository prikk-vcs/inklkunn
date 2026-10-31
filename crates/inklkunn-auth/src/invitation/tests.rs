use super::*;

#[test]
fn only_an_invitation_shaped_secret_is_worth_a_lookup() {
    let good = format!("{INVITATION_PREFIX}{}", "aB3-_".repeat(8) + "xyz");
    assert_eq!(good.len(), 4 + 43);
    assert!(looks_like_an_invitation(&good));
    assert!(!looks_like_an_invitation(&good[..good.len() - 1]));
    assert!(!looks_like_an_invitation(&format!("{good}x")));
    assert!(!looks_like_an_invitation(&good.replacen("inv_", "ink_", 1)));
    assert!(!looks_like_an_invitation(&good.replacen('a', "/", 1)));
    assert!(!looks_like_an_invitation(""));
}

#[test]
fn the_join_setting_reads_three_words_and_nothing_else() {
    assert_eq!(JoinPolicy::default(), JoinPolicy::Off);
    for policy in [JoinPolicy::Off, JoinPolicy::Provider, JoinPolicy::Anyone] {
        assert_eq!(JoinPolicy::parse(policy.as_str()), Some(policy));
    }
    for bad in ["", "Off", "on", "open", "true", " anyone", "anyone "] {
        assert_eq!(JoinPolicy::parse(bad), None, "{bad:?}");
    }
    assert!(JoinPolicy::Anyone.lets_anyone_join_with_a_password());
    assert!(!JoinPolicy::Provider.lets_anyone_join_with_a_password());
    assert!(!JoinPolicy::Off.lets_anyone_join_with_a_password());
}
