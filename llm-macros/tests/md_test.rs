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
    const GREAT: Great = include_md!(Great, "tests/great.md");
    assert_eq!(GREAT.foo, "x");
    assert_eq!(GREAT.bar, "adfslkjafd\nand some more\nlines of text\n");
    assert_eq!(GREAT.body, "Some important docuemnt\n\nblah blah blah\n");
}
