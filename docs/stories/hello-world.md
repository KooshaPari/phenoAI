# Hello World Story

<StoryHeader
    title="Your First phenotype-research-engine Operation"
    :duration="2"
    :gif="'/gifs/phenotype-research-engine-hello-world.gif'"
    difficulty="beginner"
/>

## Objective

Get phenotype-research-engine running with a basic operation.

## Prerequisites

- Rust/Node/Python installed
- phenotype-research-engine package installed

## Implementation

```rust
use phenotype-research-engine::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize client
    let client = Client::new().await?;
    
    // Execute operation
    let result = client.hello().await?;
    
    println!("Success: {}", result);
    Ok(())
}
```

## Expected Output

```
Success: Hello from phenotype-research-engine!
```

## Next Steps

- [Core Integration](./core-integration)
- Read [API Reference](../reference/api)
