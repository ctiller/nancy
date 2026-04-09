---
name: Secure Native Test Environment Isolation
description: Use `sealed_test` instead of `unsafe` standard environment primitives for configuring environment variables within test suites securely.
---

# Secure Native Test Environment Isolation

When maintaining orchestration logic containing contextually critical environment boundaries (such as `GEMINI_API_KEY` or `COORDINATOR_DID`), you **must** sandbox variable mutation within tests using the native `sealed_test` crate.

Do not use the standard library `std::env::set_var` since safe Rust bindings for mutating local operating environments concurrently have historically suffered severe security constraints globally (`set_var` is inherently marked `unsafe` as of modern Rust versions exactly due to execution isolation violations across multi-threaded harnesses).

## Anti-Pattern (Unsafe and Risky)

Never write tests leveraging global environment shifts manually via unsafe bindings:

```rust
#[test]
fn test_fast_llm_string() {
    // AVOID THIS ENTIRELY
    unsafe { std::env::set_var("GEMINI_API_KEY", "dummy_key"); }
    
    let client = fast_llm::<String>().build().unwrap();
    assert_eq!(client.model, "gemini-2.5-flash");
}
```

## Recommended Pattern (Safe and Isolated)

Bind `sealed_test` cleanly mapping to any environment conditions securely. This enforces rigid constraints isolated completely sequentially across background runners seamlessly locally:

```rust
use sealed_test::prelude::*;

#[sealed_test(env = [("GEMINI_API_KEY", "dummy_key")])]
fn test_fast_llm_string() {
    let client = fast_llm::<String>().build().unwrap();
    assert_eq!(client.model, "gemini-2.5-flash");
}
```

### Async Integration Handling

Because `sealed_test` naturally wraps standard sequential synchronous assertions, async function validation should execute bridged strictly atop localized `tokio::runtime::Runtime` structures dynamically if your implementation depends upon full event loop blocking resolution correctly. 

You can mix `#[tokio::test]` and `#[sealed_test]` cleanly, but **macro order matters**. When using them together, ensure the order is correct to properly apply environment variables within the async runtime scope.

```rust
#[tokio::test]
#[sealed_test(env = [("GEMINI_API_KEY", "dummy_key")])]
async fn test_async_function() {
    let result = some_async_logic().await;
    assert!(result.is_ok());
}
```
