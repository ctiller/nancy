// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
