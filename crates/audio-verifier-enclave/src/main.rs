#[cfg(not(feature = "dev"))]
mod cpu;

use std::env;

use audio_verifier_enclave::{MAX_BODY_BYTES_ENV, app, max_body_bytes, server_config};
use nautilus::NautilusContext;

#[cfg(not(feature = "dev"))]
const VSOCK_PORT: u32 = 3000;

#[cfg(feature = "dev")]
const LISTEN_ADDR: &str = "127.0.0.1:3000";

#[cfg(not(feature = "dev"))]
fn context() -> anyhow::Result<NautilusContext> {
    Ok(NautilusContext::from_runtime()?)
}

#[cfg(feature = "dev")]
fn context() -> anyhow::Result<NautilusContext> {
    Ok(NautilusContext::development())
}

fn main() -> anyhow::Result<()> {
    #[cfg(not(feature = "dev"))]
    cpu::require_avx512();
    audio_verifier::self_test()?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(serve())
}

async fn serve() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let max_body_bytes = max_body_bytes(&args, env::var(MAX_BODY_BYTES_ENV).ok())?;
    let ctx = context()?;
    println!("[nautilus] attestor:   {}", ctx.attestor_name());
    println!("[nautilus] public key: {}", ctx.public_key_hex());
    println!("[nautilus] address:    {}", ctx.sui_address());
    let config = server_config(max_body_bytes);
    let app = app(ctx, config.clone())?;
    println!("[server] max body bytes: {max_body_bytes}");
    #[cfg(feature = "dev")]
    {
        println!("[server] listening on http://{LISTEN_ADDR}");
        nautilus::serve(LISTEN_ADDR.parse()?, app, config).await?;
    }
    #[cfg(not(feature = "dev"))]
    {
        println!("[server] listening on vsock port {VSOCK_PORT}");
        nautilus::serve_vsock(VSOCK_PORT, app, config).await?;
    }
    Ok(())
}
