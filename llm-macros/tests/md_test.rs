use llm_macros::{include_md, md_defined};

#[md_defined]
struct Great {
    foo: string,
    bar: string,
    #[body]
    body: string,
}

#[test]
fn test_md_macro() {
    let great: Great = include_md!(Great, "tests/great.md");
    assert_eq!(great.foo, "x");
    assert_eq!(great.bar, "adfslkjafd\nand some more\nlines of text\n");
    assert_eq!(great.body, "Some important docuemnt\n\nblah blah blah\n");
}
