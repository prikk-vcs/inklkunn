#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn member(problem: &Problem) -> String {
    let body = problem.body(crate::catalog::T::new(crate::catalog::EN));
    body.get("member")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_body_is_one_json_object() {
    for bad in [
        "",
        "   ",
        "[]",
        "\"x\"",
        "7",
        "null",
        "{",
        "{} {}",
        "{\"a\":1,}",
        "{'a':1}",
    ] {
        let problem = parse(bad.as_bytes()).unwrap_err();
        assert_eq!(problem.code, Code::InvalidBody, "{bad}");
        assert_eq!(member(&problem), "$", "{bad}");
    }
    assert!(parse(b"{}").is_ok());
    assert!(parse(b" \n{ \"a\" : 1 }\t").is_ok());
}

#[test]
fn a_member_that_is_repeated_is_refused_at_any_depth() {
    for (bad, name) in [
        (r#"{"a":1,"a":2}"#, "a"),
        (r#"{"a":1,"b":{"c":1,"c":2}}"#, "c"),
        (r#"{"files":[{"path":"x","path":"y"}]}"#, "path"),
        (r#"{"ab":1,"ab":2}"#, "ab"),
        (r#"{"s":"a\"b","s":1}"#, "s"),
    ] {
        let problem = parse(bad.as_bytes()).unwrap_err();
        assert_eq!(problem.code, Code::InvalidBody, "{bad}");
        assert_eq!(member(&problem), name, "{bad}");
    }
}

#[test]
fn the_same_name_in_two_objects_is_not_a_repeat() {
    for fine in [
        r#"{"a":{"x":1},"b":{"x":2}}"#,
        r#"{"files":[{"path":"x"},{"path":"y"}]}"#,
        r#"{"a":"a","b":"a"}"#,
        r#"{"a":["a","a"],"b":[{"a":1},{"a":1}]}"#,
        r#"{"quote":"\"a\":1,\"a\":2"}"#,
    ] {
        assert!(parse(fine.as_bytes()).is_ok(), "{fine}");
    }
}

#[test]
fn members_are_taken_typed_and_what_is_left_is_unknown() {
    let mut f =
        parse(br#"{"t":"x","n":null,"b":true,"c":3,"l":["a","b"],"o":{"k":"v"},"extra":1}"#)
            .unwrap();
    assert_eq!(f.text("t").unwrap(), "x");
    assert_eq!(f.text_or_null("n").unwrap(), None);
    assert!(f.flag("b").unwrap());
    assert_eq!(f.count("c").unwrap(), 3);
    assert_eq!(f.texts("l", 2).unwrap(), ["a", "b"]);
    let mut o = f.object("o").unwrap().unwrap();
    assert_eq!(o.text("k").unwrap(), "v");
    o.end().unwrap();
    let left = f.end().unwrap_err();
    assert_eq!(member(&left), "extra");
}

#[test]
fn a_member_of_the_wrong_type_or_missing_is_named_with_its_path() {
    let mut f =
        parse(br#"{"t":1,"files":[{"path":2}],"l":["a",3],"c":-1,"d":1.5,"big":99999999999}"#)
            .unwrap();
    assert_eq!(member(&f.text("t").unwrap_err()), "t");
    assert_eq!(member(&f.text("absent").unwrap_err()), "absent");
    assert_eq!(member(&f.texts("l", 5).unwrap_err()), "l[1]");
    assert_eq!(member(&f.count("c").unwrap_err()), "c");
    assert_eq!(member(&f.count("d").unwrap_err()), "d");
    assert_eq!(member(&f.count("big").unwrap_err()), "big");
    let mut files = f.objects("files", 5).unwrap();
    let mut first = files.remove(0);
    assert_eq!(member(&first.text("path").unwrap_err()), "files[0].path");
}

#[test]
fn a_list_over_its_bound_is_refused_whole() {
    let mut f = parse(br#"{"l":["a","b","c"]}"#).unwrap();
    assert_eq!(member(&f.texts("l", 2).unwrap_err()), "l");
}

#[test]
fn what_an_answer_repeats_of_a_name_is_cut_short_and_is_data() {
    let long = "x".repeat(500);
    let body = format!(r#"{{"{long}":1}}"#);
    let f = parse(body.as_bytes()).unwrap();
    let problem = f.end().unwrap_err();
    assert_eq!(member(&problem).chars().count(), 64);
}

#[test]
fn multibyte_text_survives_reading() {
    let mut f = parse("{\"t\":\"日本語のページ 🙂 é\",\"e\":\"\\u65e5\\ud83d\\ude42\"}".as_bytes())
        .unwrap();
    assert_eq!(f.text("t").unwrap(), "日本語のページ 🙂 é");
    assert_eq!(f.text("e").unwrap(), "日🙂");
}
