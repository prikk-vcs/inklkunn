//! What follows a container directive's closing `:::`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::{NoLinks, render_html};

#[test]
fn markdown_on_the_line_right_after_a_closing_fence_is_markdown() {
    for (source, wrapper) in [
        (":::\nnext *line*\n", None),
        ("::: custom a=b\ninside\n:::\nnext *line*\n", Some("</div>")),
        ("::: note T\ninside\n:::\nnext *line*\n", Some("</aside>")),
        (
            "::: custom\n::: note\ninner\n:::\n:::\nnext *line*\n",
            Some("</div>"),
        ),
        // CRLF line ends, and the closer being the last line but one.
        (
            "::: custom\r\ninside\r\n:::\r\nnext *line*\r\n",
            Some("</div>"),
        ),
    ] {
        let html = render_html(source, &NoLinks).unsanitized_html;
        assert!(
            html.contains("<em>line</em>"),
            "{source:?} did not render the line after the closer as Markdown:\n{html}"
        );
        assert!(
            !html.contains("&lt;") && !html.contains("*line*"),
            "{source:?} shows markup as text:\n{html}"
        );
        if let Some(wrapper) = wrapper {
            // …and it is outside the wrapper, in a paragraph of its own.
            let (inside, after) = html.split_once(wrapper).expect("the wrapper is closed");
            assert!(!inside.contains("next"), "{source:?}: {html}");
            assert!(
                after.contains("<p>next <em>line</em></p>"),
                "{source:?}: {html}"
            );
        }
    }
}
